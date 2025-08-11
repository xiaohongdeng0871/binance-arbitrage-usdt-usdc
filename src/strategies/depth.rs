use super::TradingStrategy;
use crate::binance::ExchangeApi;
use crate::models::{ArbitrageOpportunity, Price, QuoteCurrency, OrderBook};
use crate::config::Config;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::sync::Arc;
use log::{debug, info, warn};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal_macros::dec;

/// 订单簿深度分析策略
/// 通过分析订单簿深度来判断市场流动性和潜在的滑点，避免在流动性不足的市场中进行套利
pub struct OrderBookDepthStrategy<T: ExchangeApi + Send + Sync> {
    config: Arc<Config>,
    api: Arc<T>,
    /// 要分析的订单簿深度（价格档位数量）
    depth_levels: usize,
    /// 最小流动性要求（以基础货币计）
    min_liquidity: Decimal,
}

impl<T: ExchangeApi + Send + Sync + 'static> OrderBookDepthStrategy<T> {
    pub fn new(config: Config, api: T, depth_levels: usize, min_liquidity: Decimal) -> Self {
        Self {
            config: Arc::new(config),
            api: Arc::new(api),
            depth_levels,
            min_liquidity,
        }
    }
    
    /// 分析订单簿深度，计算可用流动性和预期滑点
    async fn analyze_order_book_depth(&self, symbol: &str, side: &str, amount: Decimal) -> Result<(Decimal, Decimal)> {
        // 获取订单簿数据
        let order_book = self.api.get_order_book(symbol, Some(self.depth_levels as u32)).await?;
        
        // 根据交易方向选择买单或卖单
        let orders = match side {
            "buy" => &order_book.asks,  // 买入需要看卖单
            "sell" => &order_book.bids, // 卖出需要看买单
            _ => return Err(anyhow!("无效的交易方向: {}", side)),
        };
        
        if orders.is_empty() {
            return Err(anyhow!("订单簿为空"));
        }
        
        // 计算可用流动性和加权平均价格
        let mut remaining_amount = amount;
        let mut total_cost = Decimal::ZERO;
        let mut total_executed = Decimal::ZERO;
        let best_price = orders[0].0;
        
        for (price, qty) in orders {
            if remaining_amount <= Decimal::ZERO {
                break;
            }
            
            let execute_qty = if remaining_amount > *qty {
                *qty
            } else {
                remaining_amount
            };
            
            total_cost += execute_qty * (*price);
            total_executed += execute_qty;
            remaining_amount -= execute_qty;
        }
        
        // 如果无法完全执行订单，返回错误
        if remaining_amount > Decimal::ZERO {
            warn!(
                "订单簿深度不足: {} - 需要: {}, 可用: {}, 缺口: {}",
                symbol, amount, total_executed, remaining_amount
            );
            return Ok((total_executed, Decimal::ZERO));
        }
        
        // 计算加权平均价格
        let avg_price = if total_executed > Decimal::ZERO {
            total_cost / total_executed
        } else {
            best_price
        };
        
        // 计算滑点（相对于最佳价格的百分比）
        let slippage = match side {
            "buy" => (avg_price - best_price) / best_price * dec!(100),
            "sell" => (best_price - avg_price) / best_price * dec!(100),
            _ => Decimal::ZERO,
        };
        
        debug!(
            "{} 订单簿分析 - 方向: {}, 数量: {}, 加权均价: {}, 滑点: {}%",
            symbol, side, amount, avg_price, slippage
        );
        
        Ok((total_executed, slippage))
    }
}

#[async_trait]
impl<T: ExchangeApi + Send + Sync + 'static> TradingStrategy for OrderBookDepthStrategy<T> {
    fn name(&self) -> &str {
        "订单簿深度分析套利"
    }
    
    fn description(&self) -> &str {
        "通过分析订单簿深度来判断市场流动性和潜在的滑点，避免在流动性不足的市场中进行套利"
    }
    
    async fn find_opportunity(&self, base_asset: &str, usdt_price: &Price, usdc_price: &Price) -> Result<Option<ArbitrageOpportunity>> {
        let max_trade_amount = Decimal::from_f64(self.config.arbitrage_settings.max_trade_amount_usdt).unwrap();
        
        // 构造交易对名称
        let usdt_symbol = format!("{}{}", base_asset, "USDT");
        let usdc_symbol = format!("{}{}", base_asset, "USDC");
        
        // 估算交易量（以基础货币计）
        let approx_base_amount = max_trade_amount / usdt_price.price;
        
        // 分析USDT和USDC市场的订单簿深度
        let (usdt_buy_liquidity, usdt_buy_slippage) = self.analyze_order_book_depth(&usdt_symbol, "buy", approx_base_amount).await?;
        let (usdt_sell_liquidity, usdt_sell_slippage) = self.analyze_order_book_depth(&usdt_symbol, "sell", approx_base_amount).await?;
        
        let (usdc_buy_liquidity, usdc_buy_slippage) = self.analyze_order_book_depth(&usdc_symbol, "buy", approx_base_amount).await?;
        let (usdc_sell_liquidity, usdc_sell_slippage) = self.analyze_order_book_depth(&usdc_symbol, "sell", approx_base_amount).await?;
        
        // 检查流动性是否满足要求
        if usdt_buy_liquidity < self.min_liquidity || usdt_sell_liquidity < self.min_liquidity ||
           usdc_buy_liquidity < self.min_liquidity || usdc_sell_liquidity < self.min_liquidity {
            info!(
                "流动性不足，放弃套利机会 - USDT买:{}/卖:{}, USDC买:{}/卖:{}, 最小要求:{}",
                usdt_buy_liquidity, usdt_sell_liquidity, 
                usdc_buy_liquidity, usdc_sell_liquidity,
                self.min_liquidity
            );
            return Ok(None);
        }
        
        // 考虑滑点后的实际价格
        let usdt_effective_buy_price = usdt_price.price * (Decimal::ONE + usdt_buy_slippage / dec!(100));
        let usdt_effective_sell_price = usdt_price.price * (Decimal::ONE - usdt_sell_slippage / dec!(100));
        
        let usdc_effective_buy_price = usdc_price.price * (Decimal::ONE + usdc_buy_slippage / dec!(100));
        let usdc_effective_sell_price = usdc_price.price * (Decimal::ONE - usdc_sell_slippage / dec!(100));
        
        // 考虑滑点后的套利方向
        let opportunity = if usdt_effective_buy_price < usdc_effective_sell_price {
            // USDT买入，USDC卖出
            let effective_profit = (usdc_effective_sell_price - usdt_effective_buy_price) / usdt_effective_buy_price * dec!(100);
            
            info!(
                "考虑滑点后 - USDT买入({})，USDC卖出({}), 有效利润率: {}%",
                usdt_effective_buy_price, usdc_effective_sell_price, effective_profit
            );
            
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDT,
                QuoteCurrency::USDC,
                usdt_effective_buy_price,
                usdc_effective_sell_price,
                max_trade_amount,
            )
        } else if usdc_effective_buy_price < usdt_effective_sell_price {
            // USDC买入，USDT卖出
            let effective_profit = (usdt_effective_sell_price - usdc_effective_buy_price) / usdc_effective_buy_price * dec!(100);
            
            info!(
                "考虑滑点后 - USDC买入({})，USDT卖出({}), 有效利润率: {}%",
                usdc_effective_buy_price, usdt_effective_sell_price, effective_profit
            );
            
            ArbitrageOpportunity::new(
                base_asset,
                QuoteCurrency::USDC,
                QuoteCurrency::USDT,
                usdc_effective_buy_price,
                usdt_effective_sell_price,
                max_trade_amount,
            )
        } else {
            // 考虑滑点后没有套利空间
            debug!("考虑滑点后没有套利空间");
            return Ok(None);
        };
        
        Ok(Some(opportunity))
    }
    
    async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        // 验证利润是否超过最小阈值（考虑滑点影响，这里使用更高的阈值）
        let min_profit = Decimal::from_f64(self.config.arbitrage_settings.min_profit_percentage).unwrap();
        let adjusted_min_profit = min_profit * Decimal::from_f64(1.5).unwrap(); // 使用150%的阈值，因为订单簿分析已经考虑了滑点
        
        let is_valid = opportunity.profit_percentage >= adjusted_min_profit;
        
        debug!(
            "订单簿深度策略验证: 利润率 {}% {} 调整后的最小要求 {}%",
            opportunity.profit_percentage,
            if is_valid { "满足" } else { "不满足" },
            adjusted_min_profit
        );
        
        Ok(is_valid)
    }
}
