use super::TradingStrategy;
use crate::models::{ArbitrageOpportunity, Price, QuoteCurrency};
use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::sync::Arc;
use log::debug;

/// 创建简单套利策略实现，这是目前系统中使用的基本策略
/// 简单的价格差异套利策略
/// 当USDT和USDC之间的价格差异超过设定阈值时，执行套利操作
pub struct SimpleArbitrageStrategy {
    config: Arc<Config>,
}

impl SimpleArbitrageStrategy {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

#[async_trait]
impl TradingStrategy for SimpleArbitrageStrategy {
    fn name(&self) -> &str {
        "简单价格差异套利"
    }
    
    fn description(&self) -> &str {
        "当USDT和USDC交易对之间的价格差异超过设定阈值时，买入价格较低的一方，卖出价格较高的一方"
    }
    
    async fn find_opportunity(&self, base_asset: &str, usdt_price: &Price, usdc_price: &Price) -> Result<Option<ArbitrageOpportunity>> {
        let max_trade_amount = Decimal::from(self.config.arbitrage_settings.max_trade_amount_usdt);
        
        // 比较价格，确定买入和卖出方向
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
        
        debug!(
            "发现潜在套利机会: {} 买入: {} {}, 卖出: {} {}, 利润率: {}%",
            opportunity.base_asset,
            opportunity.buy_quote,
            opportunity.buy_price,
            opportunity.sell_quote,
            opportunity.sell_price,
            opportunity.profit_percentage
        );
        
        Ok(Some(opportunity))
    }
    
    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        // 验证利润是否超过最小阈值
        let min_profit = Decimal::from(self.config.arbitrage_settings.min_profit_percentage);
        let is_valid = opportunity.profit_percentage >= min_profit;
        
        debug!(
            "套利机会验证: 利润率 {}% {} 最小要求 {}%",
            opportunity.profit_percentage,
            if is_valid { "满足" } else { "不满足" },
            min_profit
        );
        
        Ok(is_valid)
    }
}
