use super::RiskController;
use crate::models::{ArbitrageOpportunity, ArbitrageResult, ArbitrageStatus};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc, Duration};
use log::{debug, info, warn};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// 交易频率控制器
/// 控制套利交易的频率，避免API限制，同时防止在短时间内执行过多交易
pub struct TradingFrequencyController {
    /// 最小交易间隔（秒）
    min_interval_seconds: i64,
    /// 单位时间最大交易次数
    max_trades_per_timeframe: usize,
    /// 时间窗口长度（秒）
    timeframe_seconds: i64,
    /// 上次交易时间
    last_trade_time: Arc<Mutex<Option<DateTime<Utc>>>>,
    /// 最近交易历史
    recent_trades: Arc<Mutex<VecDeque<DateTime<Utc>>>>,
}

impl TradingFrequencyController {
    pub fn new(min_interval_seconds: i64, max_trades_per_timeframe: usize, timeframe_seconds: i64) -> Self {
        Self {
            min_interval_seconds,
            max_trades_per_timeframe,
            timeframe_seconds,
            last_trade_time: Arc::new(Mutex::new(None)),
            recent_trades: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
    
    /// 检查交易频率是否超过限制
    fn check_frequency(&self) -> Result<(bool, Option<String>)> {
        let now = Utc::now();
        
        // 1. 检查最小交易间隔
        if let Some(last_time) = *self.last_trade_time.lock().unwrap() {
            let elapsed = now - last_time;
            if elapsed < Duration::seconds(self.min_interval_seconds) {
                let remaining = self.min_interval_seconds - elapsed.num_seconds();
                let reason = format!(
                    "交易频率过高，需等待 {} 秒",
                    remaining
                );
                debug!("{}", reason);
                return Ok((false, Some(reason)));
            }
        }
        
        // 2. 检查时间窗口内的交易次数
        let mut recent_trades = self.recent_trades.lock().unwrap();
        
        // 清除时间窗口外的交易记录
        let cutoff_time = now - Duration::seconds(self.timeframe_seconds);
        while let Some(trade_time) = recent_trades.front() {
            if *trade_time < cutoff_time {
                recent_trades.pop_front();
            } else {
                break;
            }
        }
        
        // 检查是否达到最大交易次数
        if recent_trades.len() >= self.max_trades_per_timeframe {
            let reason = format!(
                "达到时间窗口内({} 秒)最大交易次数: {}",
                self.timeframe_seconds, self.max_trades_per_timeframe
            );
            debug!("{}", reason);
            return Ok((false, Some(reason)));
        }
        
        Ok((true, None))
    }
    
    /// 记录新交易
    fn record_trade(&self) {
        let now = Utc::now();
        
        // 更新上次交易时间
        let mut last_trade_time = self.last_trade_time.lock().unwrap();
        *last_trade_time = Some(now);
        
        // 添加到最近交易记录
        let mut recent_trades = self.recent_trades.lock().unwrap();
        recent_trades.push_back(now);
        
        debug!(
            "记录交易: {}, 窗口内交易计数: {}/{}",
            now, recent_trades.len(), self.max_trades_per_timeframe
        );
    }
}

#[async_trait]
impl RiskController for TradingFrequencyController {
    fn name(&self) -> &str {
        "交易频率控制"
    }
    
    fn description(&self) -> &str {
        "控制套利交易的频率，避免API限制，同时防止在短时间内执行过多交易"
    }
    
    async fn check_opportunity(&self, _opportunity: &ArbitrageOpportunity) -> Result<(bool, Option<String>)> {
        self.check_frequency()
    }
    
    async fn record_result(&self, result: &ArbitrageResult) -> Result<()> {
        if result.status == ArbitrageStatus::Completed || result.status == ArbitrageStatus::Failed {
            // 只记录已完成或失败的交易
            self.record_trade();
            
            info!(
                "记录交易结果: {} - 状态: {:?}, 时间: {}",
                result.base_asset, result.status, Utc::now()
            );
        }
        
        Ok(())
    }
    
    async fn reset(&self) -> Result<()> {
        let mut last_trade_time = self.last_trade_time.lock().unwrap();
        *last_trade_time = None;
        
        let mut recent_trades = self.recent_trades.lock().unwrap();
        recent_trades.clear();
        
        info!("重置交易频率控制器");
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Add;
    use super::*;
    use crate::models::{QuoteCurrency};
    use rust_decimal::dec;
    
    #[tokio::test]
    async fn test_trading_frequency() {
        // 创建一个控制器，最小间隔30秒，每10分钟最多5次交易
        let controller = TradingFrequencyController::new(30, 5, 600);
        
        // 创建一个套利机会
        let opportunity = ArbitrageOpportunity::new(
            "BTC",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(50000),
            dec!(50100),
            dec!(1000),
        );
        
        // 第一次检查应该通过
        let (valid, _) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(valid);
        
        // 记录一次交易
        let result = ArbitrageResult {
            base_asset: "BTC".to_string(),
            buy_quote: "USDT".to_string(),
            sell_quote: "USDC".to_string(),
            buy_price: dec!(50000),
            sell_price: dec!(50100),
            trade_amount: dec!(0.1),
            profit: dec!(10),
            profit_percentage: dec!(0.2),
            buy_order_id: Some(1),
            sell_order_id: Some(2),
            status: ArbitrageStatus::Completed,
            start_time: Utc::now(),
            end_time: Some(Utc::now().add(Duration::seconds(29)))
        };
        
        controller.record_result(&result).await.unwrap();
        
        // 第二次检查应该失败，因为最小间隔是30秒
        let (valid, reason) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(!valid);
        assert!(reason.unwrap().contains("交易频率过高"));
        
        // 重置控制器
        controller.reset().await.unwrap();
        
        // 重置后应该可以交易
        let (valid, _) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(valid);
    }
}
