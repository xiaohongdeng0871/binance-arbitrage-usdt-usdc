use crate::config::Config;
use crate::models::{ArbitrageOpportunity, Price, QuoteCurrency};
use crate::strategies::TradingStrategy;
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// 简单价格差异套利策略
/// 
/// 当USDT和USDC交易对之间的价格差异超过设定阈值时，
/// 买入价格较低的一方，卖出价格较高的一方
pub struct SimpleArbitrageStrategy {
    config: Config,
}

impl SimpleArbitrageStrategy {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

#[async_trait]
impl TradingStrategy for SimpleArbitrageStrategy {
    fn name(&self) -> &str {
        "SimpleArbitrage"
    }

    async fn find_opportunity(
        &self,
        base_asset: &str,
        usdt_price: &Price,
        usdc_price: &Price,
    ) -> Result<Option<ArbitrageOpportunity>> {
        let price_diff = (usdc_price.price - usdt_price.price).abs();
        let avg_price = (usdt_price.price + usdc_price.price) / Decimal::from(2);
        let price_diff_pct = if !avg_price.is_zero() {
            (price_diff / avg_price) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        let min_profit_pct = Decimal::from_f64(self.config.arbitrage_settings.min_profit_percentage)
            .unwrap_or(Decimal::ZERO);
        
        // 检查价格差异是否满足最小利润要求
        if price_diff_pct < min_profit_pct {
            return Ok(None);
        }

        let max_trade_amount = Decimal::from_f64(self.config.arbitrage_settings.max_trade_amount_usdt)
            .unwrap_or(Decimal::ZERO);
        
        let opportunity = if usdt_price.price < usdc_price.price {
            // USDT买入，USDC卖出
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDT,
                QuoteCurrency::USDC,
                usdt_price.price,
                usdc_price.price,
                max_trade_amount,
            )
        } else {
            // USDC买入，USDT卖出
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDC,
                QuoteCurrency::USDT,
                usdc_price.price,
                usdt_price.price,
                max_trade_amount,
            )
        };

        Ok(Some(opportunity))
    }

    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        let price_diff_pct = opportunity.profit_percentage;
        let min_profit_pct = Decimal::from_f64(self.config.arbitrage_settings.min_profit_percentage)
            .unwrap_or(Decimal::ZERO);
        
        Ok(price_diff_pct >= min_profit_pct)
    }
}