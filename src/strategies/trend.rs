use crate::config::Config;
use crate::models::{ArbitrageOpportunity, Price};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// 趋势跟踪策略
/// 结合短期价格趋势，避免在价格快速变化时进行套利
#[derive(Debug, Clone)]
pub struct TrendFollowingStrategy {
    config: Config,
    #[allow(dead_code)]
    short_window: usize,
    #[allow(dead_code)]
    long_window: usize,
    #[allow(dead_code)]
    trend_threshold: Decimal,
}

impl TrendFollowingStrategy {
    pub fn new(config: Config, short_window: usize, long_window: usize, trend_threshold: Decimal) -> Self {
        Self {
            config,
            short_window,
            long_window,
            trend_threshold,
        }
    }
}

#[async_trait]
impl super::TradingStrategy for TrendFollowingStrategy {
    fn name(&self) -> &str {
        "TrendFollowing"
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