use crate::models::{ArbitrageOpportunity, ArbitrageResult, QuoteCurrency};
use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc, NaiveTime};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::collections::HashMap;

/// 风险控制组件接口
#[async_trait]
pub trait RiskController: Send + Sync {
    /// 控制器名称
    fn name(&self) -> &str;
    
    /// 控制器描述
    fn description(&self) -> &str;
    
    /// 检查套利机会是否可以执行
    /// 返回: 是否可以执行, 拒绝原因(如果不可执行)
    async fn check_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<(bool, Option<String>)>;
    
    /// 记录套利结果
    async fn record_result(&self, result: &ArbitrageResult) -> Result<()>;
    
    /// 重置风险控制器状态
    async fn reset(&self) -> Result<()>;
}

/// 风控管理器，集成多个风险控制组件
pub struct RiskManager {
    config: Arc<Config>,
    controllers: Vec<Box<dyn RiskController>>,
}

impl RiskManager {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            controllers: Vec::new(),
        }
    }
    
    /// 添加风控组件
    pub fn add_controller<T: RiskController + 'static>(&mut self, controller: T) {
        self.controllers.push(Box::new(controller));
    }
    
    /// 检查套利机会是否通过所有风控规则
    pub async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<(bool, Vec<String>)> {
        let mut is_valid = true;
        let mut rejection_reasons = Vec::new();
        
        for controller in &self.controllers {
            match controller.check_opportunity(opportunity).await {
                Ok((valid, reason)) => {
                    if !valid {
                        is_valid = false;
                        if let Some(reason_str) = reason {
                            rejection_reasons.push(format!("{}: {}", controller.name(), reason_str));
                        }
                    }
                },
                Err(e) => {
                    rejection_reasons.push(format!("{}: 风控检查错误 - {}", controller.name(), e));
                    is_valid = false;
                }
            }
        }
        
        Ok((is_valid, rejection_reasons))
    }
    
    /// 记录套利结果
    pub async fn record_result(&self, result: &ArbitrageResult) -> Result<()> {
        for controller in &self.controllers {
            controller.record_result(result).await?;
        }
        
        Ok(())
    }
    
    /// 重置所有风控组件
    pub async fn reset_all(&self) -> Result<()> {
        for controller in &self.controllers {
            controller.reset().await?;
        }
        
        Ok(())
    }
}

// 导出各种风控组件
pub mod loss_limit;
pub mod price_protection;
pub mod exposure;
pub mod time_window;
pub mod frequency;
pub mod blacklist;

// 重导出风控组件
pub use loss_limit::DailyLossLimitController;
pub use price_protection::AbnormalPriceController;
pub use exposure::ExposureController;
pub use time_window::TradingTimeWindowController;
pub use frequency::TradingFrequencyController;
pub use blacklist::PairBlacklistController;
