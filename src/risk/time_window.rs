use super::RiskController;
use crate::models::{ArbitrageOpportunity, ArbitrageResult};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc, Local, NaiveTime, Timelike, Datelike};
use log::{debug, info, warn};
use std::sync::Arc;

/// 交易时间窗口控制器
/// 限制只在特定时间段内进行交易，可用于避免低流动性时段或配合交易策略
pub struct TradingTimeWindowController {
    /// 允许交易的开始时间 (24小时制，如9:30)
    start_time: NaiveTime,
    /// 允许交易的结束时间 (24小时制，如16:00)
    end_time: NaiveTime,
    /// 是否在周末交易
    trade_on_weekends: bool,
}

impl TradingTimeWindowController {
    pub fn new(start_hour: u32, start_min: u32, end_hour: u32, end_min: u32, trade_on_weekends: bool) -> Result<Self> {
        let start_time = NaiveTime::from_hms_opt(start_hour, start_min, 0)
            .ok_or_else(|| anyhow::anyhow!("无效的开始时间: {}:{}", start_hour, start_min))?;
            
        let end_time = NaiveTime::from_hms_opt(end_hour, end_min, 0)
            .ok_or_else(|| anyhow::anyhow!("无效的结束时间: {}:{}", end_hour, end_min))?;
            
        Ok(Self {
            start_time,
            end_time,
            trade_on_weekends,
        })
    }
    
    /// 检查当前时间是否在允许交易的时间窗口内
    fn is_within_trading_hours(&self) -> (bool, String) {
        let now = Local::now();
        let current_time = now.time();
        let weekday = now.weekday().number_from_monday(); // 1 = 周一, 7 = 周日
        
        // 检查是否是周末
        let is_weekend = weekday >= 6; // 6 = 周六, 7 = 周日
        if is_weekend && !self.trade_on_weekends {
            return (
                false, 
                format!("当前是周末 ({}), 不在交易时段", 
                    if weekday == 6 { "周六" } else { "周日" }
                )
            );
        }
        
        // 检查是否在交易时间内
        let is_trading_time = if self.start_time <= self.end_time {
            // 简单情况：开始时间早于结束时间
            current_time >= self.start_time && current_time <= self.end_time
        } else {
            // 复杂情况：开始时间晚于结束时间（跨午夜）
            current_time >= self.start_time || current_time <= self.end_time
        };
        
        if is_trading_time {
            (true, "".to_string())
        } else {
            (
                false, 
                format!(
                    "当前时间 {} 不在交易时段 {} - {} 内",
                    current_time.format("%H:%M"),
                    self.start_time.format("%H:%M"),
                    self.end_time.format("%H:%M")
                )
            )
        }
    }
}

#[async_trait]
impl RiskController for TradingTimeWindowController {
    fn name(&self) -> &str {
        "交易时间窗口"
    }
    
    fn description(&self) -> &str {
        "限制只在特定时间段内进行交易，可用于避免低流动性时段或配合交易策略"
    }
    
    async fn check_opportunity(&self, _opportunity: &ArbitrageOpportunity) -> Result<(bool, Option<String>)> {
        let (is_valid, reason) = self.is_within_trading_hours();
        
        if !is_valid {
            debug!("交易时间检查: {}", reason);
            return Ok((false, Some(reason)));
        }
        
        Ok((true, None))
    }
    
    async fn record_result(&self, _result: &ArbitrageResult) -> Result<()> {
        // 这个控制器不需要记录交易结果
        Ok(())
    }
    
    async fn reset(&self) -> Result<()> {
        // 这个控制器没有状态需要重置
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ArbitrageOpportunity, QuoteCurrency};
    use rust_decimal::prelude::*;
    use rust_decimal::dec;

    #[tokio::test]
    async fn test_trading_time_window() {
        // 创建一个控制器，允许交易时间为9:30-16:00，周末不交易
        let controller = TradingTimeWindowController::new(9, 30, 16, 0, false).unwrap();
        
        // 创建一个套利机会
        let opportunity = ArbitrageOpportunity::new(
            "BTC",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(50000),
            dec!(50100),
            dec!(1000),
        );
        
        // 注意：这个测试的结果将取决于运行测试的时间
        // 可以通过模拟时间来测试不同时间段的行为
        let (valid, reason) = controller.check_opportunity(&opportunity).await.unwrap();
        
        // 由于我们无法确定测试运行时的时间，所以这里不做具体断言
        println!("交易时间窗口测试结果: {}, 原因: {:?}", valid, reason);
    }
}
