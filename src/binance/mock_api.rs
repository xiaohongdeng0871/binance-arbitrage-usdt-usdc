use crate::binance::ExchangeApi;
use crate::models::{OrderBook, Price, QuoteCurrency, Side, Symbol, OrderInfo, OrderStatus};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::{Decimal,dec};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use log::{debug, info, warn};

/// 模拟币安API，用于测试和开发
#[derive(Debug,Clone)]
pub struct MockBinanceApi {
    prices: Arc<Mutex<HashMap<String, Decimal>>>,
    balances: Arc<Mutex<HashMap<String, Decimal>>>,
    orders: Arc<Mutex<HashMap<u64, OrderInfo>>>,
    next_order_id: Arc<Mutex<u64>>,
}

impl MockBinanceApi {
    pub fn new() -> Self {
        let mut prices = HashMap::new();
        // 模拟BTC/USDT和BTC/USDC的初始价格，添加一点差异以便能够进行套利
        prices.insert("BTCUSDT".to_string(), dec!(50000.00));
        prices.insert("BTCUSDC".to_string(), dec!(50025.00));
        prices.insert("ETHUSDT".to_string(), dec!(3000.00));
        prices.insert("ETHUSDC".to_string(), dec!(3002.50));
        
        let mut balances = HashMap::new();
        // 设置初始余额
        balances.insert("USDT".to_string(), dec!(10000.00));
        balances.insert("USDC".to_string(), dec!(10000.00));
        balances.insert("BTC".to_string(), dec!(1.0));
        balances.insert("ETH".to_string(), dec!(10.0));
        
        Self {
            prices: Arc::new(Mutex::new(prices)),
            balances: Arc::new(Mutex::new(balances)),
            orders: Arc::new(Mutex::new(HashMap::new())),
            next_order_id: Arc::new(Mutex::new(1)),
        }
    }
    
    /// 更新模拟价格
    pub fn update_price(&self, symbol: &str, price: Decimal) {
        let mut prices = self.prices.lock().unwrap();
        prices.insert(symbol.to_string(), price);
    }
    
    /// 获取当前时间戳（毫秒）
    fn get_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
    
    /// 解析交易对，获取基础资产和报价资产
    fn parse_symbol(&self, symbol: &str) -> Result<(String, String)> {
        if symbol.ends_with("USDT") {
            let base = symbol.strip_suffix("USDT").unwrap_or_default();
            Ok((base.to_string(), "USDT".to_string()))
        } else if symbol.ends_with("USDC") {
            let base = symbol.strip_suffix("USDC").unwrap_or_default();
            Ok((base.to_string(), "USDC".to_string()))
        } else {
            Err(anyhow!("不支持的交易对格式: {}", symbol))
        }
    }
}

#[async_trait]
impl ExchangeApi for MockBinanceApi {
    async fn get_symbol_info(&self, symbol: &str) -> Result<Symbol> {
        let (base_asset, quote_asset) = self.parse_symbol(symbol)?;
        
        Ok(Symbol {
            base_asset,
            quote_asset,
            min_notional: dec!(10.0),
            min_qty: dec!(0.0001),
            step_size: dec!(0.0001),
            tick_size: dec!(0.01),
        })
    }
    
    async fn get_price(&self, symbol: &str) -> Result<Price> {
        let prices = self.prices.lock().unwrap();
        
        if let Some(price) = prices.get(symbol) {
            Ok(Price {
                symbol: symbol.to_string(),
                price: *price,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow!("价格不可用: {}", symbol))
        }
    }
    
    async fn get_order_book(&self, symbol: &str, _limit: Option<u32>) -> Result<OrderBook> {
        let price = {
            let prices = self.prices.lock().unwrap();
            *prices.get(symbol).ok_or_else(|| anyhow!("价格不可用: {}", symbol))?
        };
        
        // 模拟订单簿，围绕当前价格创建买卖盘
        let mut bids = Vec::new();
        let mut asks = Vec::new();
        
        // 创建10个买单，价格依次降低
        for i in 1..=10 {
            let bid_price = price * Decimal::from(1000 - i) / Decimal::from(1000);
            let qty = Decimal::from(i) / Decimal::from(10);
            bids.push((bid_price, qty));
        }
        
        // 创建10个卖单，价格依次升高
        for i in 1..=10 {
            let ask_price = price * Decimal::from(1000 + i) / Decimal::from(1000);
            let qty = Decimal::from(i) / Decimal::from(10);
            asks.push((ask_price, qty));
        }
        
        Ok(OrderBook {
            symbol: symbol.to_string(),
            bids,
            asks,
            timestamp: Utc::now(),
        })
    }
    
    async fn place_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let (base_asset, quote_asset) = self.parse_symbol(symbol)?;
        
        // 获取当前价格
        let current_price = {
            let prices = self.prices.lock().unwrap();
            *prices.get(symbol).ok_or_else(|| anyhow!("价格不可用: {}", symbol))?
        };
        
        // 使用指定价格或者当前市场价格
        let execution_price = price.unwrap_or(current_price);
        
        // 计算总价值
        let total_value = quantity * execution_price;
        
        // 检查余额
        {
            let mut balances = self.balances.lock().unwrap();
            
            match side {
                Side::Buy => {
                    // 买入需要检查报价资产余额
                    let balance = balances.get(&quote_asset).cloned().unwrap_or_default();
                    if balance < total_value {
                        return Err(anyhow!("余额不足: {} < {}", balance, total_value));
                    }
                    
                    // 扣除报价资产，增加基础资产
                    *balances.entry(quote_asset.clone()).or_insert(Decimal::ZERO) -= total_value;
                    *balances.entry(base_asset.clone()).or_insert(Decimal::ZERO) += quantity;
                },
                Side::Sell => {
                    // 卖出需要检查基础资产余额
                    let balance = balances.get(&base_asset).cloned().unwrap_or_default();
                    if balance < quantity {
                        return Err(anyhow!("余额不足: {} < {}", balance, quantity));
                    }
                    
                    // 扣除基础资产，增加报价资产
                    *balances.entry(base_asset.clone()).or_insert(Decimal::ZERO) -= quantity;
                    *balances.entry(quote_asset.clone()).or_insert(Decimal::ZERO) += total_value;
                }
            }
        }
        
        // 创建订单
        let order_id = {
            let mut next_id = self.next_order_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };
        
        let order = OrderInfo {
            order_id,
            symbol: symbol.to_string(),
            price: execution_price,
            qty: quantity,
            side,
            status: OrderStatus::Filled,  // 模拟环境中，订单立即成交
            timestamp: Utc::now(),
        };
        
        // 保存订单
        {
            let mut orders = self.orders.lock().unwrap();
            orders.insert(order_id, order.clone());
        }
        
        info!("Mock API: 订单已执行 - ID: {}, 交易对: {}, 方向: {:?}, 价格: {}, 数量: {}", 
            order_id, symbol, side, execution_price, quantity);
        
        Ok(order)
    }
    
    async fn get_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let orders = self.orders.lock().unwrap();
        
        if let Some(order) = orders.get(&order_id) {
            if order.symbol == symbol {
                Ok(order.clone())
            } else {
                Err(anyhow!("订单ID和交易对不匹配"))
            }
        } else {
            Err(anyhow!("订单不存在: {}", order_id))
        }
    }
    
    async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let mut orders = self.orders.lock().unwrap();
        
        if let Some(mut order) = orders.get(&order_id).cloned() {
            if order.symbol == symbol {
                // 如果订单已经完成，则无法取消
                if order.status == OrderStatus::Filled {
                    return Err(anyhow!("无法取消已成交的订单"));
                }
                
                order.status = OrderStatus::Cancelled;
                orders.insert(order_id, order.clone());
                
                Ok(order)
            } else {
                Err(anyhow!("订单ID和交易对不匹配"))
            }
        } else {
            Err(anyhow!("订单不存在: {}", order_id))
        }
    }
    
    async fn get_account_balance(&self, asset: &str) -> Result<Decimal> {
        let balances = self.balances.lock().unwrap();
        
        if let Some(balance) = balances.get(asset) {
            Ok(*balance)
        } else {
            Ok(Decimal::ZERO)  // 如果资产不存在，返回零余额
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    #[tokio::test]
    async fn test_mock_api() {
        let api = MockBinanceApi::new();
        
        // 测试获取价格
        let btcusdt_price = api.get_price("BTCUSDT").await.unwrap();
        assert_eq!(btcusdt_price.price, dec!(50000.00));
        
        // 测试下单和余额变化
        let initial_usdt = api.get_account_balance("USDT").await.unwrap();
        let initial_btc = api.get_account_balance("BTC").await.unwrap();
        
        // 买入0.1 BTC
        let buy_order = api.place_order("BTCUSDT", Side::Buy, dec!(0.1), None).await.unwrap();
        assert_eq!(buy_order.status, OrderStatus::Filled);
        
        // 检查余额变化
        let after_buy_usdt = api.get_account_balance("USDT").await.unwrap();
        let after_buy_btc = api.get_account_balance("BTC").await.unwrap();
        
        assert_eq!(after_buy_usdt, initial_usdt - dec!(0.1) * dec!(50000.00));
        assert_eq!(after_buy_btc, initial_btc + dec!(0.1));
        
        // 卖出0.05 BTC
        let sell_order = api.place_order("BTCUSDT", Side::Sell, dec!(0.05), None).await.unwrap();
        assert_eq!(sell_order.status, OrderStatus::Filled);
        
        // 检查余额变化
        let after_sell_usdt = api.get_account_balance("USDT").await.unwrap();
        let after_sell_btc = api.get_account_balance("BTC").await.unwrap();
        
        assert_eq!(after_sell_usdt, after_buy_usdt + dec!(0.05) * dec!(50000.00));
        assert_eq!(after_sell_btc, after_buy_btc - dec!(0.05));
    }
}
