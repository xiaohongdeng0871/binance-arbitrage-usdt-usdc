use super::TradingStrategy;
use crate::models::{ArbitrageOpportunity, Price, QuoteCurrency};
use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::sync::Arc;
use log::{debug, info, warn};
use rust_decimal_macros::dec;
use std::sync::Mutex;
use std::collections::VecDeque;
use chrono::{DateTime, Utc};
use mysql_common::bigdecimal::num_traits::real::Real;

/// 滑点控制策略
/// 通过控制下单时的价格滑点，避免在价格波动较大的市场中产生亏损
pub struct SlippageControlStrategy {
    config: Arc<Config>,
    /// 最大允许的滑点百分比
    max_slippage_pct: Decimal,
    /// 历史价格波动率窗口大小
    volatility_window_size: usize,
    /// 历史价格数据
    price_history: Arc<Mutex<VecDeque<(DateTime<Utc>, Decimal, Decimal)>>>,
}

impl SlippageControlStrategy {
    pub fn new(config: Config, max_slippage_pct: Decimal, volatility_window_size: usize) -> Self {
        Self {
            config: Arc::new(config),
            max_slippage_pct,
            volatility_window_size,
            price_history: Arc::new(Mutex::new(VecDeque::with_capacity(volatility_window_size + 1))),
        }
    }
    
    /// 记录价格历史
    fn record_price(&self, usdt_price: Decimal, usdc_price: Decimal) {
        let now = Utc::now();
        let mut history = self.price_history.lock().unwrap();
        
        // 添加新价格
        history.push_back((now, usdt_price, usdc_price));
        
        // 保持窗口大小
        if history.len() > self.volatility_window_size {
            history.pop_front();
        }
    }
    
    /// 计算价格波动率（过去N个价格点的标准差/均值）
    fn calculate_volatility(&self) -> (Decimal, Decimal) {
        let history = self.price_history.lock().unwrap();
        
        if history.len() < 2 {
            return (Decimal::ZERO, Decimal::ZERO);
        }
        
        // 计算USDT价格的统计数据
        let usdt_prices: Vec<Decimal> = history.iter().map(|(_, usdt, _)| *usdt).collect();
        let usdt_mean = usdt_prices.iter().sum::<Decimal>() / Decimal::from(usdt_prices.len());
        
        let usdt_variance_sum = usdt_prices.iter()
            .map(|p| (*p - usdt_mean).powu(2))
            .sum::<Decimal>();
            
        let usdt_std_dev = (usdt_variance_sum / Decimal::from(usdt_prices.len() - 1))
            .sqrt()
            .unwrap_or(Decimal::ZERO);
            
        let usdt_volatility = if usdt_mean.is_zero() {
            Decimal::ZERO
        } else {
            usdt_std_dev / usdt_mean * dec!(100)
        };
        
        // 计算USDC价格的统计数据
        let usdc_prices: Vec<Decimal> = history.iter().map(|(_, _, usdc)| *usdc).collect();
        let usdc_mean = usdc_prices.iter().sum::<Decimal>() / Decimal::from(usdc_prices.len());
        
        let usdc_variance_sum = usdc_prices.iter()
            .map(|p| (*p - usdc_mean).powu(2))
            .sum::<Decimal>();
            
        let usdc_std_dev = (usdc_variance_sum / Decimal::from(usdc_prices.len() - 1))
            .sqrt()
            .unwrap_or(Decimal::ZERO);
            
        let usdc_volatility = if usdc_mean.is_zero() {
            Decimal::ZERO
        } else {
            usdc_std_dev / usdc_mean * dec!(100)
        };
        
        (usdt_volatility, usdc_volatility)
    }
    
    /// 根据波动率调整滑点控制
    fn adjust_for_volatility(&self, opportunity: &mut ArbitrageOpportunity) -> Decimal {
        let (usdt_vol, usdc_vol) = self.calculate_volatility();
        
        // 使用较大的波动率作为参考
        let max_vol = if usdt_vol > usdc_vol { usdt_vol } else { usdc_vol };
        
        // 基于波动率调整价格
        // 如果波动率高，我们需要设置更严格的价格限制，避免成交价格大幅偏离预期
        let volatility_factor = Decimal::ONE + (max_vol / dec!(100));
        
        // 根据交易方向调整价格
        match (opportunity.buy_quote, opportunity.sell_quote) {
            (QuoteCurrency::USDT, QuoteCurrency::USDC) => {
                // 买入价格略低，卖出价格略高
                opportunity.buy_price = opportunity.buy_price * (Decimal::ONE - self.max_slippage_pct / dec!(100) / volatility_factor);
                opportunity.sell_price = opportunity.sell_price * (Decimal::ONE + self.max_slippage_pct / dec!(100) / volatility_factor);
            },
            (QuoteCurrency::USDC, QuoteCurrency::USDT) => {
                // 买入价格略低，卖出价格略高
                opportunity.buy_price = opportunity.buy_price * (Decimal::ONE - self.max_slippage_pct / dec!(100) / volatility_factor);
                opportunity.sell_price = opportunity.sell_price * (Decimal::ONE + self.max_slippage_pct / dec!(100) / volatility_factor);
            },
            _ => {}
        }
        
        // 重新计算利润率
        let price_diff = opportunity.sell_price - opportunity.buy_price;
        let profit_percentage = if opportunity.buy_price.is_zero() {
            Decimal::ZERO
        } else {
            (price_diff / opportunity.buy_price) * Decimal::from(100)
        };
        
        opportunity.price_diff = price_diff;
        opportunity.profit_percentage = profit_percentage;
        
        max_vol
    }
}

#[async_trait]
impl TradingStrategy for SlippageControlStrategy {
    fn name(&self) -> &str {
        "滑点控制套利策略"
    }
    
    fn description(&self) -> &str {
        "通过控制下单时的价格滑点，在波动较大的市场中保护套利交易"
    }
    
    async fn find_opportunity(&self, base_asset: &str, usdt_price: &Price, usdc_price: &Price) -> Result<Option<ArbitrageOpportunity>> {
        // 记录价格历史
        self.record_price(usdt_price.price, usdc_price.price);
        
        let max_trade_amount = Decimal::from(self.config.arbitrage_settings.max_trade_amount_usdt);
        
        // 基于当前价格创建潜在的套利机会
        let mut opportunity = if usdt_price.price < usdc_price.price {
            // USDT买入，USDC卖出
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDT,
                QuoteCurrency::USDC,
                usdt_price.price,
                usdc_price.price,
                max_trade_amount,
            )
        } else {
            // USDC买入，USDT卖出
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDC,
                QuoteCurrency::USDT,
                usdc_price.price,
                usdt_price.price,
                max_trade_amount,
            )
        };
        
        // 调整价格以考虑滑点
        let volatility = self.adjust_for_volatility(&mut opportunity);
        
        info!(
            "滑点调整后的套利机会: {} 买入: {} {}, 卖出: {} {}, 波动率: {}%, 调整后利润率: {}%",
            opportunity.base_asset,
            opportunity.buy_quote,
            opportunity.buy_price,
            opportunity.sell_quote,
            opportunity.sell_price,
            volatility,
            opportunity.profit_percentage
        );
        
        Ok(Some(opportunity))
    }
    
    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        let min_profit = Decimal::from(self.config.arbitrage_settings.min_profit_percentage);
        
        // 根据波动率调整最小利润要求
        let (usdt_vol, usdc_vol) = self.calculate_volatility();
        let max_vol = if usdt_vol > usdc_vol { usdt_vol } else { usdc_vol };
        
        // 波动率越高，要求的利润率越高
        let volatility_factor = Decimal::ONE + (max_vol / dec!(20)); // 每5%的波动率增加20%的利润要求
        let adjusted_min_profit = min_profit * volatility_factor;
        
        let is_valid = opportunity.profit_percentage >= adjusted_min_profit;
        
        debug!(
            "滑点策略验证: 利润率 {}%, 波动率 {}%, {} 调整后的最小要求 {}%",
            opportunity.profit_percentage,
            max_vol,
            if is_valid { "满足" } else { "不满足" },
            adjusted_min_profit
        );
        
        Ok(is_valid)
    }
}
