use super::RiskController;
use crate::models::{ArbitrageOpportunity, ArbitrageResult};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc, Duration};
use log::{debug, info, warn, error};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// 价格记录
#[derive(Debug, Clone)]
struct PriceRecord {
    timestamp: DateTime<Utc>,
    symbol: String,
    price: Decimal,
}

/// 异常价格保护控制器
/// 检测极端价格波动，暂停交易以防止在异常市场条件下交易
pub struct AbnormalPriceController {
    /// 价格历史记录
    price_history: Arc<Mutex<VecDeque<PriceRecord>>>,
    /// 窗口大小（保留的价格记录数量）
    window_size: usize,
    /// 异常价格变化阈值（百分比）
    abnormal_threshold: Decimal,
    /// 冷却期（秒），在检测到异常后暂停交易的时间
    cooldown_period: i64,
    /// 最后一次异常检测时间
    last_abnormal_time: Arc<Mutex<Option<DateTime<Utc>>>>,
}

impl AbnormalPriceController {
    pub fn new(window_size: usize, abnormal_threshold: Decimal, cooldown_period: i64) -> Self {
        Self {
            price_history: Arc::new(Mutex::new(VecDeque::with_capacity(window_size * 2))),
            window_size,
            abnormal_threshold,
            cooldown_period,
            last_abnormal_time: Arc::new(Mutex::new(None)),
        }
    }
    
    /// 添加价格记录
    pub fn add_price(&self, symbol: &str, price: Decimal) {
        let record = PriceRecord {
            timestamp: Utc::now(),
            symbol: symbol.to_string(),
            price,
        };
        
        let mut history = self.price_history.lock().unwrap();
        
        // 添加新记录
        history.push_back(record);
        
        // 保持窗口大小
        if history.len() > self.window_size * 2 {  // 为每个交易对保留window_size个记录
            history.pop_front();
        }
    }
    
    /// 检测异常价格变化
    fn detect_abnormal_price(&self, symbol: &str) -> Option<Decimal> {
        let history = self.price_history.lock().unwrap();
        
        // 获取指定交易对的价格记录
        let symbol_records: Vec<_> = history.iter()
            .filter(|record| record.symbol == symbol)
            .collect();
            
        if symbol_records.len() < 2 {
            return None;  // 没有足够的数据进行分析
        }
        
        // 计算最新价格相对于过去价格的变化
        let latest = symbol_records.last().unwrap();
        let previous_records = &symbol_records[..symbol_records.len() - 1];
        
        // 计算过去价格的平均值
        let sum: Decimal = previous_records.iter().map(|r| r.price).sum();
        let avg_price = sum / Decimal::from(previous_records.len());
        
        if avg_price.is_zero() {
            return None;
        }
        
        // 计算价格变化百分比
        let change_pct = ((latest.price - avg_price) / avg_price).abs() * dec!(100);
        
        if change_pct > self.abnormal_threshold {
            Some(change_pct)
        } else {
            None
        }
    }
    
    /// 检查是否在冷却期内
    fn is_in_cooldown(&self) -> bool {
        if let Some(last_time) = *self.last_abnormal_time.lock().unwrap() {
            let elapsed = Utc::now() - last_time;
            
            // 如果冷却期尚未结束
            if elapsed < Duration::seconds(self.cooldown_period) {
                let remaining = self.cooldown_period - elapsed.num_seconds();
                debug!("仍在冷却期内，剩余 {} 秒", remaining);
                return true;
            }
        }
        
        false
    }
}

#[async_trait]
impl RiskController for AbnormalPriceController {
    fn name(&self) -> &str {
        "异常价格保护"
    }
    
    fn description(&self) -> &str {
        "检测极端价格波动，暂停交易以防止在异常市场条件下交易"
    }
    
    async fn check_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<(bool, Option<String>)> {
        // 构造交易对名称
        let usdt_symbol = format!("{}{}", opportunity.base_asset, "USDT");
        let usdc_symbol = format!("{}{}", opportunity.base_asset, "USDC");
        
        // 添加价格记录
        match opportunity.buy_quote {
            crate::models::QuoteCurrency::USDT => {
                self.add_price(&usdt_symbol, opportunity.buy_price);
                self.add_price(&usdc_symbol, opportunity.sell_price);
            },
            crate::models::QuoteCurrency::USDC => {
                self.add_price(&usdc_symbol, opportunity.buy_price);
                self.add_price(&usdt_symbol, opportunity.sell_price);
            },
        }
        
        // 检查是否在冷却期内
        if self.is_in_cooldown() {
            let reason = "仍在异常价格冷却期内，暂停交易".to_string();
            return Ok((false, Some(reason)));
        }
        
        // 检测异常价格
        if let Some(change_pct) = self.detect_abnormal_price(&usdt_symbol) {
            let reason = format!(
                "检测到 {} 异常价格变化: {:.2}% > 阈值 {:.2}%",
                usdt_symbol, change_pct, self.abnormal_threshold
            );
            warn!("{}", reason);
            
            // 设置冷却期
            *self.last_abnormal_time.lock().unwrap() = Some(Utc::now());
            
            return Ok((false, Some(reason)));
        }
        
        if let Some(change_pct) = self.detect_abnormal_price(&usdc_symbol) {
            let reason = format!(
                "检测到 {} 异常价格变化: {:.2}% > 阈值 {:.2}%",
                usdc_symbol, change_pct, self.abnormal_threshold
            );
            warn!("{}", reason);
            
            // 设置冷却期
            *self.last_abnormal_time.lock().unwrap() = Some(Utc::now());
            
            return Ok((false, Some(reason)));
        }
        
        Ok((true, None))
    }
    
    async fn record_result(&self, result: &ArbitrageResult) -> Result<()> {
        // 这个控制器不需要记录交易结果
        Ok(())
    }
    
    async fn reset(&self) -> Result<()> {
        let mut history = self.price_history.lock().unwrap();
        history.clear();
        
        let mut last_time = self.last_abnormal_time.lock().unwrap();
        *last_time = None;
        
        info!("重置异常价格保护控制器");
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuoteCurrency, ArbitrageStatus};
    
    #[tokio::test]
    async fn test_abnormal_price_detection() {
        let controller = AbnormalPriceController::new(5, dec!(10), 60);
        
        // 添加正常价格
        controller.add_price("BTCUSDT", dec!(50000));
        controller.add_price("BTCUSDT", dec!(50100));
        controller.add_price("BTCUSDT", dec!(50200));
        
        // 创建一个正常的套利机会
        let opportunity = ArbitrageOpportunity::new(
            "BTC",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(50300),
            dec!(50400),
            dec!(1000),
        );
        
        // 应该通过检查
        let (valid, _) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(valid);
        
        // 添加一个异常价格
        controller.add_price("BTCUSDT", dec!(60000));  // 20%的波动
        
        // 创建一个新的套利机会
        let opportunity = ArbitrageOpportunity::new(
            "BTC",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(60000),
            dec!(60100),
            dec!(1000),
        );
        
        // 应该被拒绝
        let (valid, reason) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(!valid);
        assert!(reason.unwrap().contains("检测到 BTCUSDT 异常价格变化"));
    }
}
