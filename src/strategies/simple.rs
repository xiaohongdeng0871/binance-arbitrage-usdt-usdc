use crate::config::Config;
use crate::models::{ArbitrageOpportunity, Price};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// 简单价格差异套利策略
/// 基于USDT和USDC交易对之间的直接价格差异进行套利
#[derive(Debug, Clone)]
pub struct SimpleArbitrageStrategy {
    config: Config,
}

impl SimpleArbitrageStrategy {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

#[async_trait]
impl super::TradingStrategy for SimpleArbitrageStrategy {
    fn name(&self) -> &str {
        "Simple"
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