use crate::models::{Price, OrderBook, ArbitrageOpportunity, QuoteCurrency};
use crate::config::Config;
use async_trait::async_trait;
use anyhow::Result;
use rust_decimal::Decimal;
use std::sync::Arc;

/// 交易策略接口
#[async_trait]
pub trait TradingStrategy: Send + Sync {
    /// 策略名称
    fn name(&self) -> &str;
    
    /// 策略描述
    fn description(&self) -> &str;
    
    /// 分析市场数据，寻找套利机会
    async fn find_opportunity(&self, base_asset: &str, usdt_price: &Price, usdc_price: &Price) -> Result<Option<ArbitrageOpportunity>>;
    
    /// 验证套利机会是否符合策略要求
    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool>;
}

pub mod simple;
pub mod twap;
pub mod depth;
pub mod slippage;
pub mod trend;

// 重导出所有策略
pub use simple::SimpleArbitrageStrategy;
pub use twap::TimeWeightedAverageStrategy;
pub use depth::OrderBookDepthStrategy;
pub use slippage::SlippageControlStrategy;
pub use trend::TrendFollowingStrategy;
