use super::RiskController;
use crate::models::{ArbitrageOpportunity, ArbitrageResult, ArbitrageStatus};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc, Local, Datelike};
use log::{debug, info, warn};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::{Arc, Mutex};

/// 每日亏损限制控制器
/// 限制每日最大亏损金额，超过限制后停止交易
pub struct DailyLossLimitController {
    /// 每日最大亏损金额
    max_daily_loss: Decimal,
    /// 当前日期
    current_date: Arc<Mutex<(i32, u32, u32)>>, // (year, month, day)
    /// 当日累计盈亏
    daily_pnl: Arc<Mutex<Decimal>>,
}

impl DailyLossLimitController {
    pub fn new(max_daily_loss: Decimal) -> Self {
        let now = Local::now();
        let current_date = (now.year(), now.month(), now.day());
        
        Self {
            max_daily_loss,
            current_date: Arc::new(Mutex::new(current_date)),
            daily_pnl: Arc::new(Mutex::new(dec!(0))),
        }
    }
    
    /// 检查是否为新的一天，如果是则重置盈亏统计
    fn check_new_day(&self) {
        let now = Local::now();
        let today = (now.year(), now.month(), now.day());
        
        let mut current_date = self.current_date.lock().unwrap();
        
        if *current_date != today {
            // 新的一天，重置盈亏
            info!("新的交易日开始: {:04}-{:02}-{:02}, 重置日盈亏统计", today.0, today.1, today.2);
            *current_date = today;
            
            let mut daily_pnl = self.daily_pnl.lock().unwrap();
            *daily_pnl = dec!(0);
        }
    }
}

#[async_trait]
impl RiskController for DailyLossLimitController {
    fn name(&self) -> &str {
        "每日亏损限制"
    }
    
    fn description(&self) -> &str {
        "限制每日最大亏损金额，超过限制后停止交易"
    }
    
    async fn check_opportunity(&self, _opportunity: &ArbitrageOpportunity) -> Result<(bool, Option<String>)> {
        // 检查是否为新的一天
        self.check_new_day();
        
        // 检查当前亏损是否超过限制
        let daily_pnl = *self.daily_pnl.lock().unwrap();
        
        if daily_pnl <= -self.max_daily_loss {
            let reason = format!(
                "已达到每日最大亏损限额: {:.2}，今日累计: {:.2}",
                self.max_daily_loss, daily_pnl
            );
            warn!("{}", reason);
            return Ok((false, Some(reason)));
        }
        
        Ok((true, None))
    }
    
    async fn record_result(&self, result: &ArbitrageResult) -> Result<()> {
        // 检查是否为新的一天
        self.check_new_day();
        
        // 只记录已完成的交易
        if result.status == ArbitrageStatus::Completed {
            let mut daily_pnl = self.daily_pnl.lock().unwrap();
            *daily_pnl += result.profit;
            
            info!(
                "记录套利结果: {} 利润: {:.2}, 当日累计: {:.2}",
                result.base_asset, result.profit, *daily_pnl
            );
        }
        
        Ok(())
    }
    
    async fn reset(&self) -> Result<()> {
        let mut daily_pnl = self.daily_pnl.lock().unwrap();
        *daily_pnl = dec!(0);
        
        info!("重置每日亏损限制控制器");
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuoteCurrency, ArbitrageStatus};
    use chrono::Utc;
    
    #[tokio::test]
    async fn test_daily_loss_limit() {
        let controller = DailyLossLimitController::new(dec!(100));
        
        // 创建一个套利机会
        let opportunity = ArbitrageOpportunity::new(
            "BTC",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(50000),
            dec!(50100),
            dec!(1000),
        );
        
        // 初始状态应该通过检查
        let (valid, _) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(valid);
        
        // 创建一个亏损的结果
        let loss_result = ArbitrageResult {
            base_asset: "BTC".to_string(),
            buy_quote: "USDT".to_string(),
            sell_quote: "USDC".to_string(),
            buy_price: dec!(50000),
            sell_price: dec!(49900),
            trade_amount: dec!(0.1),
            profit: dec!(-50),
            profit_percentage: dec!(-0.1),
            buy_order_id: Some(1),
            sell_order_id: Some(2),
            status: ArbitrageStatus::Completed,
            timestamp: Utc::now(),
        };
        
        // 记录亏损
        controller.record_result(&loss_result).await.unwrap();
        
        // 应该还能通过检查
        let (valid, _) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(valid);
        
        // 再记录一次相同的亏损（总亏损100）
        controller.record_result(&loss_result).await.unwrap();
        
        // 应该刚好达到限额，但还能通过
        let (valid, _) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(valid);
        
        // 再记录一次亏损（总亏损150）
        controller.record_result(&loss_result).await.unwrap();
        
        // 现在应该被拒绝
        let (valid, reason) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(!valid);
        assert!(reason.unwrap().contains("已达到每日最大亏损限额"));
        
        // 重置后应该又能通过
        controller.reset().await.unwrap();
        let (valid, _) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(valid);
    }
}
