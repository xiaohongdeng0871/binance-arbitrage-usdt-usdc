use super::TradingStrategy;
use crate::models::{ArbitrageOpportunity, Price, QuoteCurrency};
use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use rust_decimal::dec;
use std::sync::Arc;
use log::{debug, info};
use std::sync::Mutex;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::*;

/// 时间加权平均价格（TWAP）策略
/// 将一个大的套利订单分解成多个小订单，在特定时间段内均匀执行
/// 这可以减少市场冲击，并降低在波动市场中的风险
pub struct TimeWeightedAverageStrategy {
    config: Arc<Config>,
    /// 分割的订单数量
    slices: usize,
    /// 每个分割订单之间的间隔（秒）
    interval_seconds: u64,
    /// 价格历史记录
    price_history: Arc<Mutex<Vec<(DateTime<Utc>, Decimal, Decimal)>>>,
}

impl TimeWeightedAverageStrategy {
    pub fn new(config: Config, slices: usize, interval_seconds: u64) -> Self {
        Self {
            config: Arc::new(config),
            slices,
            interval_seconds,
            price_history: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// 记录价格历史
    pub fn record_price(&self, usdt_price: Decimal, usdc_price: Decimal) {
        let now = Utc::now();
        let mut history = self.price_history.lock().unwrap();
        
        // 添加新价格
        history.push((now, usdt_price, usdc_price));
        
        // 只保留最近100个价格点
        if history.len() > 100 {
            history.remove(0);
        }
    }
    
    /// 计算时间加权平均价格
    fn calculate_twap(&self, duration_seconds: i64) -> Option<(Decimal, Decimal)> {
        let history = self.price_history.lock().unwrap();
        if history.is_empty() {
            return None;
        }
        
        let cutoff_time = Utc::now() - Duration::seconds(duration_seconds);
        
        // 过滤出指定时间范围内的价格
        let relevant_prices: Vec<_> = history
            .iter()
            .filter(|(time, _, _)| *time >= cutoff_time)
            .collect();
            
        if relevant_prices.is_empty() {
            return None;
        }
        
        // 计算TWAP
        let sum_usdt: Decimal = relevant_prices.iter().map(|(_, usdt, _)| *usdt).sum();
        let sum_usdc: Decimal = relevant_prices.iter().map(|(_, _, usdc)| *usdc).sum();
        let count = Decimal::from(relevant_prices.len());
        
        let twap_usdt = sum_usdt / count;
        let twap_usdc = sum_usdc / count;
        
        Some((twap_usdt, twap_usdc))
    }
}

#[async_trait]
impl TradingStrategy for TimeWeightedAverageStrategy {
    fn name(&self) -> &str {
        "时间加权平均价格(TWAP)套利"
    }
    
    fn description(&self) -> &str {
        "将套利订单分割成多个小订单在一段时间内执行，减少市场冲击并降低风险"
    }
    
    async fn find_opportunity(&self, base_asset: &str, usdt_price: &Price, usdc_price: &Price) -> Result<Option<ArbitrageOpportunity>> {
        // 记录最新价格
        self.record_price(usdt_price.price, usdc_price.price);
        
        // 计算TWAP (过去5分钟)
        let twap = self.calculate_twap(300);
        
        // 如果没有足够的历史数据，使用当前价格
        let (twap_usdt, twap_usdc) = match twap {
            Some((usdt, usdc)) => (usdt, usdc),
            None => (usdt_price.price, usdc_price.price),
        };
        
        debug!(
            "当前价格 - USDT: {}, USDC: {}; TWAP - USDT: {}, USDC: {}",
            usdt_price.price, usdc_price.price, twap_usdt, twap_usdc
        );
        
        // 计算每个分片的交易金额
        let total_amount = Decimal::from_f64(self.config.arbitrage_settings.max_trade_amount_usdt).unwrap();
        let slice_amount = total_amount / Decimal::from(self.slices);
        
        // 比较TWAP价格，确定买入和卖出方向
        let opportunity = if twap_usdt < twap_usdc {
            // USDT买入，USDC卖出
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDT,
                QuoteCurrency::USDC,
                twap_usdt,
                twap_usdc,
                slice_amount, // 注意这里用的是分片金额
            )
        } else {
            // USDC买入，USDT卖出
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDC,
                QuoteCurrency::USDT,
                twap_usdc,
                twap_usdt,
                slice_amount, // 注意这里用的是分片金额
            )
        };
        
        info!(
            "TWAP套利机会 - {} 买入: {} {}, 卖出: {} {}, 分片数: {}, 每片金额: {}, 总金额: {}",
            opportunity.base_asset,
            opportunity.buy_quote,
            opportunity.buy_price,
            opportunity.sell_quote,
            opportunity.sell_price,
            self.slices,
            slice_amount,
            total_amount
        );
        
        Ok(Some(opportunity))
    }
    
    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        // 验证利润是否超过最小阈值
        let min_profit = Decimal::from_f64(self.config.arbitrage_settings.min_profit_percentage).unwrap();
        
        // TWAP策略可能需要较低的利润阈值，因为它降低了风险
        let adjusted_min_profit = min_profit * Decimal::from_f64(0.8).unwrap(); // 使用80%的阈值
        
        let is_valid = opportunity.profit_percentage >= adjusted_min_profit;
        
        debug!(
            "TWAP套利机会验证: 利润率 {}% {} 调整后的最小要求 {}%",
            opportunity.profit_percentage,
            if is_valid { "满足" } else { "不满足" },
            adjusted_min_profit
        );
        
        Ok(is_valid)
    }
}
