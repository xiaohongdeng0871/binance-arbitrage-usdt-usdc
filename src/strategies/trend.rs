use super::TradingStrategy;
use crate::models::{ArbitrageOpportunity, Price, QuoteCurrency};
use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::{Decimal,dec};
use std::sync::Arc;
use log::{debug, info, warn};
use std::sync::Mutex;
use std::collections::VecDeque;
use chrono::{DateTime, Utc, Duration};
use rust_decimal::prelude::*;

/// 趋势类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrendDirection {
    Up,
    Down,
    Sideways,
}

impl std::fmt::Display for TrendDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrendDirection::Up => write!(f, "上涨"),
            TrendDirection::Down => write!(f, "下跌"),
            TrendDirection::Sideways => write!(f, "横盘"),
        }
    }
}

/// 趋势跟踪策略
/// 分析短期价格趋势，避免在价格波动的不利方向进行套利
pub struct TrendFollowingStrategy {
    config: Arc<Config>,
    /// 价格历史
    price_history: Arc<Mutex<VecDeque<(DateTime<Utc>, Decimal, Decimal)>>>,
    /// 短期趋势窗口（数据点数量）
    short_window: usize,
    /// 长期趋势窗口（数据点数量）
    long_window: usize,
    /// 趋势判断阈值（百分比）
    trend_threshold: Decimal,
}

impl TrendFollowingStrategy {
    pub fn new(
        config: Config,
        short_window: usize,
        long_window: usize,
        trend_threshold: Decimal
    ) -> Self {
        Self {
            config: Arc::new(config),
            price_history: Arc::new(Mutex::new(VecDeque::with_capacity(long_window + 1))),
            short_window,
            long_window,
            trend_threshold,
        }
    }
    
    /// 记录价格历史
    fn record_price(&self, usdt_price: Decimal, usdc_price: Decimal) {
        let now = Utc::now();
        let mut history = self.price_history.lock().unwrap();
        
        // 添加新价格
        history.push_back((now, usdt_price, usdc_price));
        
        // 保持窗口大小
        if history.len() > self.long_window {
            history.pop_front();
        }
    }
    
    /// 计算趋势方向和强度
    fn calculate_trend(&self, is_usdt: bool) -> (TrendDirection, Decimal) {
        let history = self.price_history.lock().unwrap();
        
        if history.len() < self.short_window {
            return (TrendDirection::Sideways, Decimal::ZERO);
        }
        
        // 获取价格数据
        let prices: Vec<Decimal> = if is_usdt {
            history.iter().map(|(_, usdt, _)| *usdt).collect()
        } else {
            history.iter().map(|(_, _, usdc)| *usdc).collect()
        };
        
        // 计算短期均价（最近N个数据点）
        let short_window_prices = prices.iter().rev().take(self.short_window);
        let short_mean: Decimal = short_window_prices.clone().sum::<Decimal>() / Decimal::from(self.short_window);
        
        // 如果数据点足够，计算长期均价
        if history.len() >= self.long_window {
            let long_window_prices = prices.iter().rev().take(self.long_window);
            let long_mean: Decimal = long_window_prices.sum::<Decimal>() / Decimal::from(self.long_window);
            
            // 计算趋势变化百分比
            let trend_change = ((short_mean - long_mean) / long_mean) * dec!(100);
            
            // 根据阈值判断趋势方向
            let direction = if trend_change > self.trend_threshold {
                TrendDirection::Up
            } else if trend_change < -self.trend_threshold {
                TrendDirection::Down
            } else {
                TrendDirection::Sideways
            };
            
            (direction, trend_change.abs())
        } else {
            // 数据不足，无法确定长期趋势
            (TrendDirection::Sideways, Decimal::ZERO)
        }
    }
    
    /// 检查是否有最近的价格异常波动
    fn has_recent_volatility_spike(&self, minutes: i64) -> bool {
        let history = self.price_history.lock().unwrap();
        if history.len() < 2 {
            return false;
        }
        
        let cutoff_time = Utc::now() - Duration::minutes(minutes);
        let recent_prices: Vec<_> = history
            .iter()
            .filter(|(time, _, _)| *time >= cutoff_time)
            .collect();
            
        if recent_prices.len() < 2 {
            return false;
        }
        
        // 计算最大价格变化
        let mut max_change_pct = Decimal::ZERO;
        
        for i in 1..recent_prices.len() {
            let (_, prev_usdt, prev_usdc) = recent_prices[i-1];
            let (_, curr_usdt, curr_usdc) = recent_prices[i];
            
            // 计算USDT价格变化百分比
            if !prev_usdt.is_zero() {
                let change_pct = ((curr_usdt - prev_usdt) / prev_usdt).abs() * dec!(100);
                if change_pct > max_change_pct {
                    max_change_pct = change_pct;
                }
            }
            
            // 计算USDC价格变化百分比
            if !prev_usdc.is_zero() {
                let change_pct = ((curr_usdc - prev_usdc) / prev_usdc).abs() * dec!(100);
                if change_pct > max_change_pct {
                    max_change_pct = change_pct;
                }
            }
        }
        
        // 5%的价格波动被视为异常波动
        max_change_pct > dec!(5.0)
    }
}

#[async_trait]
impl TradingStrategy for TrendFollowingStrategy {
    fn name(&self) -> &str {
        "趋势跟踪套利策略"
    }
    
    fn description(&self) -> &str {
        "分析短期价格趋势，避免在价格波动的不利方向进行套利"
    }
    
    async fn find_opportunity(&self, base_asset: &str, usdt_price: &Price, usdc_price: &Price) -> Result<Option<ArbitrageOpportunity>> {
        // 记录价格历史
        self.record_price(usdt_price.price, usdc_price.price);
        
        // 检查是否有最近的异常波动，如果有则避免交易
        if self.has_recent_volatility_spike(5) {  // 检查过去5分钟
            warn!("检测到最近的异常价格波动，暂停套利操作");
            return Ok(None);
        }
        
        // 计算USDT和USDC的趋势
        let (usdt_trend, usdt_strength) = self.calculate_trend(true);
        let (usdc_trend, usdc_strength) = self.calculate_trend(false);
        
        info!(
            "趋势分析: USDT {}({:.2}%), USDC {}({:.2}%)",
            usdt_trend, usdt_strength, usdc_trend, usdc_strength
        );
        
        // 基于趋势做出决策
        let max_trade_amount = Decimal::from_f64(self.config.arbitrage_settings.max_trade_amount_usdt).unwrap();
        
        // 套利方向决策
        let mut opportunity = if usdt_price.price < usdc_price.price {
            // 正常情况: USDT买入，USDC卖出
            
            // 但如果USDT趋势强烈上升或USDC强烈下降，可能不是好时机
            if (usdt_trend == TrendDirection::Up && usdt_strength > dec!(2.0)) || 
               (usdc_trend == TrendDirection::Down && usdc_strength > dec!(2.0)) {
                warn!(
                    "不利趋势: USDT上涨({:.2}%), USDC下跌({:.2}%), 可能导致套利失败",
                    usdt_strength, usdc_strength
                );
                return Ok(None);
            }
            
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDT,
                QuoteCurrency::USDC,
                usdt_price.price,
                usdc_price.price,
                max_trade_amount,
            )
        } else {
            // 正常情况: USDC买入，USDT卖出
            
            // 但如果USDC趋势强烈上升或USDT强烈下降，可能不是好时机
            if (usdc_trend == TrendDirection::Up && usdc_strength > dec!(2.0)) || 
               (usdt_trend == TrendDirection::Down && usdt_strength > dec!(2.0)) {
                warn!(
                    "不利趋势: USDC上涨({:.2}%), USDT下跌({:.2}%), 可能导致套利失败",
                    usdc_strength, usdt_strength
                );
                return Ok(None);
            }
            
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDC,
                QuoteCurrency::USDT,
                usdc_price.price,
                usdt_price.price,
                max_trade_amount,
            )
        };
        
        // 根据趋势强度调整交易量
        // 如果趋势变化较大，减少交易量以降低风险
        let usdt_usdc_strength = usdt_strength.max(usdc_strength);
        if usdt_usdc_strength > dec!(1.0) {
            let reduction_factor = dec!(1.0) - (usdt_usdc_strength - dec!(1.0)) / dec!(10.0);
            let adjusted_amount = opportunity.max_trade_amount * reduction_factor;
            opportunity.max_trade_amount = if adjusted_amount < (max_trade_amount / dec!(5.0)) {
                max_trade_amount / dec!(5.0)  // 最多减少到原始金额的20%
            } else {
                adjusted_amount
            };
            
            info!(
                "由于趋势波动，调整交易金额: {:.2} (原始: {:.2}, 减少因子: {:.2})",
                opportunity.max_trade_amount, max_trade_amount, reduction_factor
            );
        }
        
        Ok(Some(opportunity))
    }
    
    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        // 获取最小利润阈值
        let min_profit = Decimal::from_f64(self.config.arbitrage_settings.min_profit_percentage).unwrap();
        
        // 在趋势强烈的情况下，增加最小利润要求
        let (usdt_trend, usdt_strength) = self.calculate_trend(true);
        let (usdc_trend, usdc_strength) = self.calculate_trend(false);
        
        let trend_strength = usdt_strength.max(usdc_strength);
        
        // 基于趋势强度调整最小利润要求
        let profit_multiplier = Decimal::ONE + (trend_strength / dec!(10.0));
        let adjusted_min_profit = min_profit * profit_multiplier;
        
        let is_valid = opportunity.profit_percentage >= adjusted_min_profit;
        
        debug!(
            "趋势策略验证: 利润率 {:.2}%, 趋势强度 {:.2}%, {} 调整后的最小要求 {:.2}%",
            opportunity.profit_percentage,
            trend_strength,
            if is_valid { "满足" } else { "不满足" },
            adjusted_min_profit
        );
        
        Ok(is_valid)
    }
}
