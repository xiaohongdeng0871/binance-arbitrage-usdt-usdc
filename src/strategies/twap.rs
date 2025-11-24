use crate::config::Config;
use crate::models::{ArbitrageOpportunity, Price};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// 时间加权平均价格策略
/// 将大订单分解为小订单在一段时间内执行，以减少市场冲击
#[derive(Debug, Clone)]
pub struct TimeWeightedAverageStrategy {
    config: Config,
    #[allow(dead_code)]
    slices: usize,
    #[allow(dead_code)]
    interval_seconds: u64,
}

impl TimeWeightedAverageStrategy {
    pub fn new(config: Config, slices: usize, interval_seconds: u64) -> Self {
        Self {
            config,
            slices,
            interval_seconds,
        }
    }
}

#[async_trait]
impl super::TradingStrategy for TimeWeightedAverageStrategy {
    fn name(&self) -> &str {
        "TWAP"
    }

    async fn find_opportunity(
        &self,
        base_asset: &str,
        usdt_price: &Price,
        usdc_price: &Price,
    ) -> Result<Option<ArbitrageOpportunity>> {
        let max_trade_amount = Decimal::from_f64(self.config.arbitrage_settings.max_trade_amount_usdt).unwrap_or(Decimal::ZERO);
        
        let opportunity = ArbitrageOpportunity::new(
            base_asset,
            crate::models::QuoteCurrency::USDT,
            crate::models::QuoteCurrency::USDC,
            usdt_price.price,
            usdc_price.price,
            max_trade_amount,
        );
        
        Ok(Some(opportunity))
    }

    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        let min_profit = Decimal::from_f64(self.config.arbitrage_settings.min_profit_percentage).unwrap_or(Decimal::ZERO);
        Ok(opportunity.profit_percentage >= min_profit)
    }
}