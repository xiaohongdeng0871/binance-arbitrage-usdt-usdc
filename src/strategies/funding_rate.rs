use crate::models::{ArbitrageOpportunity, Price};
use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// 资金费率套利策略
#[derive(Debug, Clone)]
pub struct FundingRateArbitrageStrategy {
    config: Config,
    #[allow(dead_code)]
    min_funding_rate_diff: Decimal,
}

impl FundingRateArbitrageStrategy {
    pub fn new(config: Config, min_funding_rate_diff: Decimal) -> Self {
        Self {
            config,
            min_funding_rate_diff,
        }
    }
}

#[async_trait]
impl crate::strategies::TradingStrategy for FundingRateArbitrageStrategy {
    fn name(&self) -> &str {
        "FundingRateArbitrage"
    }

    async fn find_opportunity(
        &self,
        base_asset: &str,
        spot_price: &Price,
        futures_price: &Price,
    ) -> Result<Option<ArbitrageOpportunity>> {
        // 对于资金费率套利，机会发现逻辑在执行阶段处理
        // 这里我们创建一个基本的机会对象
        let max_trade_amount = Decimal::from_f64(self.config.arbitrage_settings.max_trade_amount_usdt).unwrap_or(Decimal::ZERO);
        
        let opportunity = ArbitrageOpportunity::new(
            base_asset,
            crate::models::QuoteCurrency::USDT,
            crate::models::QuoteCurrency::USDT,
            spot_price.price,
            futures_price.price,
            max_trade_amount,
        );
        
        Ok(Some(opportunity))
    }

    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        // 基本验证
        let min_profit = Decimal::from_f64(self.config.arbitrage_settings.min_profit_percentage).unwrap_or(Decimal::ZERO);
        Ok(opportunity.profit_percentage >= min_profit)
    }
}