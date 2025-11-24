mod simple;
mod twap;
mod depth;
mod slippage;
mod trend;
mod funding_rate;

pub use simple::SimpleArbitrageStrategy;
pub use twap::TimeWeightedAverageStrategy;
pub use depth::OrderBookDepthStrategy;
pub use slippage::SlippageControlStrategy;
pub use trend::TrendFollowingStrategy;
pub use funding_rate::FundingRateArbitrageStrategy;

use crate::models::{ArbitrageOpportunity, Price};
use anyhow::Result;
use async_trait::async_trait;

/// 交易策略 trait
/// 
/// 所有交易策略都需要实现这个 trait
#[async_trait]
pub trait TradingStrategy: Send + Sync {
    /// 获取策略名称
    #[allow(dead_code)]
    fn name(&self) -> &str;
    
    /// 寻找套利机会
    /// 
    /// # 参数
    /// * `base_asset` - 基础资产（如 BTC、ETH）
    /// * `usdt_price` - USDT 交易对的价格
    /// * `usdc_price` - USDC 交易对的价格
    /// 
    /// # 返回值
    /// 返回找到的套利机会，如果没有则返回 None
    async fn find_opportunity(
        &self,
        base_asset: &str,
        usdt_price: &Price,
        usdc_price: &Price,
    ) -> Result<Option<ArbitrageOpportunity>>;
    
    /// 验证套利机会是否满足策略要求
    /// 
    /// # 参数
    /// * `opportunity` - 待验证的套利机会
    /// 
    /// # 返回值
    /// 返回验证结果，true 表示满足要求，false 表示不满足
    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool>;
}