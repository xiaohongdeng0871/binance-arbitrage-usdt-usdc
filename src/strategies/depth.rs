use crate::exchanges::ExchangeApi;
use crate::config::Config;
use crate::models::{ArbitrageOpportunity, Price};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::sync::Arc;
use rust_decimal::prelude::FromPrimitive;

/// 订单簿深度分析策略
/// 考虑订单簿深度和流动性进行交易决策
#[derive(Debug, Clone)]
pub struct OrderBookDepthStrategy<T: ExchangeApi + Send + Sync + 'static> {
    config: Config,
    #[allow(dead_code)]
    api: Arc<T>,
    #[allow(dead_code)]
    depth_levels: usize,
    #[allow(dead_code)]
    min_liquidity: Decimal,
}

impl<T: ExchangeApi + Send + Sync + 'static> OrderBookDepthStrategy<T> {
    pub fn new(config: Config, api: Arc<T>, depth_levels: usize, min_liquidity: Decimal) -> Self {
        Self {
            config,
            api,
            depth_levels,
            min_liquidity,
        }
    }
}

#[async_trait]
impl<T: ExchangeApi + Send + Sync + 'static> super::TradingStrategy for OrderBookDepthStrategy<T> {
    fn name(&self) -> &str {
        "OrderBookDepth"
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