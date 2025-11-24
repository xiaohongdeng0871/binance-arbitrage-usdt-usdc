use crate::models::{OrderBook, Price, Side, Symbol, OrderInfo, FundingRate};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use rust_decimal::prelude::FromPrimitive;
use chrono::Utc;

/// 交易所枚举类型
#[derive(Debug, Clone, PartialEq)]
pub enum Exchange {
    Binance,
    Okx,
    GateIo,
    Bitget,
    Kucoin,
}

impl Exchange {
    /// 将交易所枚举转换为字符串形式
    pub fn as_str(&self) -> &'static str {
        match self {
            Exchange::Binance => "binance",
            Exchange::Okx => "okx",
            Exchange::GateIo => "gate.io",
            Exchange::Bitget => "bitget",
            Exchange::Kucoin => "kucoin",
        }
    }
    
    /// 从字符串解析交易所枚举
    pub fn from_str(exchange: &str) -> Option<Exchange> {
        match exchange.to_lowercase().as_str() {
            "binance" => Some(Exchange::Binance),
            "okx" => Some(Exchange::Okx),
            "gate.io" | "gateio" => Some(Exchange::GateIo),
            "bitget" => Some(Exchange::Bitget),
            "kucoin" => Some(Exchange::Kucoin),
            _ => None,
        }
    }
}

/// 交易所API接口，定义了与交易所交互所需的各种操作
#[async_trait]
pub trait ExchangeApi: Send + Sync {
    /// 获取交易所类型
    fn get_exchange(&self) -> Exchange;
    
    /// 获取交易对信息
    /// 注意：此方法当前未被使用，但作为API接口的一部分保留
    #[allow(dead_code)]
    async fn get_symbol_info(&self, symbol: &str) -> Result<Symbol>;
    
    /// 获取指定交易对的最新价格
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称，例如 "BTCUSDT"
    /// 
    /// # 返回值
    /// 返回包含价格信息的 [Price](crate::models::Price) 结构体
    async fn get_price(&self, symbol: &str) -> Result<Price>;
    
    /// 获取现货价格
    /// 
    /// # 参数
    /// * `symbol` - 现货交易对名称
    /// 
    /// # 返回值
    /// 返回包含价格信息的 [Price](crate::models::Price) 结构体
    async fn get_spot_price(&self, symbol: &str) -> Result<Price>;
    
    /// 获取合约价格
    /// 
    /// # 参数
    /// * `symbol` - 合约交易对名称
    /// 
    /// # 返回值
    /// 返回包含价格信息的 [Price](crate::models::Price) 结构体
    async fn get_futures_price(&self, symbol: &str) -> Result<Price>;
    
    /// 获取指定交易对的订单簿
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称
    /// * `limit` - 可选参数，限制返回的订单数量
    /// 
    /// # 返回值
    /// 返回包含买卖盘信息的 [OrderBook](crate::models::OrderBook) 结构体
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook>;
    
    /// 获取指定交易对的资金费率
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称
    /// 
    /// # 返回值
    /// 返回包含资金费率信息的 [FundingRate](crate::models::FundingRate) 结构体
    async fn get_funding_rate(&self, symbol: &str) -> Result<FundingRate>;
    
    /// 下单（现货）
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称
    /// * `side` - 订单方向（买入或卖出）
    /// * `quantity` - 订单数量
    /// * `price` - 可选参数，订单价格（市价单不需要）
    /// 
    /// # 返回值
    /// 返回包含订单信息的 [OrderInfo](crate::models::OrderInfo) 结构体
    async fn place_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo>;
    
    /// 下单（合约）
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称
    /// * `side` - 订单方向（买入或卖出）
    /// * `quantity` - 订单数量
    /// * `price` - 可选参数，订单价格（市价单不需要）
    /// 
    /// # 返回值
    /// 返回包含订单信息的 [OrderInfo](crate::models::OrderInfo) 结构体
    async fn place_futures_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo>;
    
    /// 查询订单状态（现货）
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称
    /// * `order_id` - 订单ID
    /// 
    /// # 返回值
    /// 返回包含订单详细信息的 [OrderInfo](crate::models::OrderInfo) 结构体
    async fn get_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo>;
    
    /// 查询订单状态（合约）
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称
    /// * `order_id` - 订单ID
    /// 
    /// # 返回值
    /// 返回包含订单详细信息的 [OrderInfo](crate::models::OrderInfo) 结构体
    async fn get_futures_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo>;
    
    /// 撤销订单（现货）
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称
    /// * `order_id` - 订单ID
    /// 
    /// # 返回值
    /// 返回撤销后的订单信息 [OrderInfo](crate::models::OrderInfo) 结构体
    async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo>;
    
    /// 撤销订单（合约）
    /// 
    /// # 参数
    /// * `symbol` - 交易对名称
    /// * `order_id` - 订单ID
    /// 
    /// # 返回值
    /// 返回撤销后的订单信息 [OrderInfo](crate::models::OrderInfo) 结构体
    async fn cancel_futures_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo>;
    
    /// 获取账户指定资产余额（现货）
    /// 
    /// # 参数
    /// * `asset` - 资产名称，如 "USDT"
    /// 
    /// # 返回值
    /// 返回该资产的可用余额
    async fn get_account_balance(&self, asset: &str) -> Result<Decimal>;
    
    /// 获取账户指定资产余额（合约）
    /// 
    /// # 参数
    /// * `asset` - 资产名称，如 "USDT"
    /// 
    /// # 返回值
    /// 返回该资产的可用余额
    #[allow(dead_code)]
    async fn get_futures_account_balance(&self, asset: &str) -> Result<Decimal>;
}

/// 通用交易所模拟API，用于测试和开发
#[derive(Debug, Clone)]
pub struct MockExchangeApi {
    exchange: Exchange,
    prices: Arc<Mutex<HashMap<String, Decimal>>>,
    balances: Arc<Mutex<HashMap<String, Decimal>>>,
    futures_balances: Arc<Mutex<HashMap<String, Decimal>>>,
    orders: Arc<Mutex<HashMap<u64, OrderInfo>>>,
    futures_orders: Arc<Mutex<HashMap<u64, OrderInfo>>>,
    next_order_id: Arc<Mutex<u64>>,
    funding_rates: Arc<Mutex<HashMap<String, Decimal>>>,
}

impl MockExchangeApi {
    pub fn new(exchange: Exchange) -> Self {
        let mut prices = HashMap::new();
        // 模拟BTC/USDT的初始价格
        prices.insert("BTCUSDT".to_string(), Decimal::from_f64(50000.00).unwrap());
        
        let mut balances = HashMap::new();
        // 设置初始余额
        balances.insert("USDT".to_string(), Decimal::from_f64(10000.00).unwrap());
        balances.insert("BTC".to_string(), Decimal::from_f64(1.0).unwrap());
        
        let mut futures_balances = HashMap::new();
        // 设置初始合约账户余额
        futures_balances.insert("USDT".to_string(), Decimal::from_f64(10000.00).unwrap());
        futures_balances.insert("BTC".to_string(), Decimal::from_f64(1.0).unwrap());
        
        let mut funding_rates = HashMap::new();
        // 设置初始资金费率
        funding_rates.insert("BTCUSDT".to_string(), Decimal::from_f64(0.0001).unwrap()); // 0.01%
        
        Self {
            exchange,
            prices: Arc::new(Mutex::new(prices)),
            balances: Arc::new(Mutex::new(balances)),
            futures_balances: Arc::new(Mutex::new(futures_balances)),
            orders: Arc::new(Mutex::new(HashMap::new())),
            futures_orders: Arc::new(Mutex::new(HashMap::new())),
            next_order_id: Arc::new(Mutex::new(1)),
            funding_rates: Arc::new(Mutex::new(funding_rates)),
        }
    }
    
    /// 更新模拟价格
    pub fn update_price(&self, symbol: &str, price: Decimal) {
        let mut prices = self.prices.lock().unwrap();
        prices.insert(symbol.to_string(), price);
    }
    
    /// 更新模拟余额（用于测试）
    #[cfg(test)]
    pub fn update_balance(&self, asset: &str, balance: Decimal) {
        let mut balances = self.balances.lock().unwrap();
        balances.insert(asset.to_string(), balance);
    }
    
    /// 解析交易对，获取基础资产和报价资产
    fn parse_symbol(&self, symbol: &str) -> Result<(String, String)> {
        if symbol.ends_with("USDT") {
            let base = symbol.strip_suffix("USDT").unwrap_or_default();
            Ok((base.to_string(), "USDT".to_string()))
        } else {
            Err(anyhow::anyhow!("不支持的交易对格式: {}", symbol))
        }
    }
}

#[async_trait]
impl ExchangeApi for MockExchangeApi {
    fn get_exchange(&self) -> Exchange {
        self.exchange.clone()
    }
    
    async fn get_symbol_info(&self, symbol: &str) -> Result<Symbol> {
        let (base_asset, quote_asset) = self.parse_symbol(symbol)?;
        
        Ok(Symbol {
            base_asset,
            quote_asset,
            min_notional: Decimal::from_f64(10.0).unwrap(),
            min_qty: Decimal::from_f64(0.0001).unwrap(),
            step_size: Decimal::from_f64(0.0001).unwrap(),
            tick_size: Decimal::from_f64(0.01).unwrap(),
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
            Err(anyhow::anyhow!("价格不可用: {}", symbol))
        }
    }
    
    async fn get_spot_price(&self, symbol: &str) -> Result<Price> {
        // 模拟环境中现货和合约价格相同
        self.get_price(symbol).await
    }
    
    async fn get_futures_price(&self, symbol: &str) -> Result<Price> {
        // 模拟环境中现货和合约价格相同
        self.get_price(symbol).await
    }
    
    async fn get_order_book(&self, symbol: &str, _limit: Option<u32>) -> Result<OrderBook> {
        let price = {
            let prices = self.prices.lock().unwrap();
            *prices.get(symbol).ok_or_else(|| anyhow::anyhow!("价格不可用: {}", symbol))?
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
    
    async fn get_funding_rate(&self, symbol: &str) -> Result<FundingRate> {
        let funding_rates = self.funding_rates.lock().unwrap();
        
        if let Some(funding_rate) = funding_rates.get(symbol) {
            Ok(FundingRate {
                symbol: symbol.to_string(),
                funding_rate: *funding_rate,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow::anyhow!("资金费率不可用: {}", symbol))
        }
    }
    
    async fn place_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let (base_asset, quote_asset) = self.parse_symbol(symbol)?;
        
        // 获取当前价格
        let current_price = {
            let prices = self.prices.lock().unwrap();
            *prices.get(symbol).ok_or_else(|| anyhow::anyhow!("价格不可用: {}", symbol))?
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
                        return Err(anyhow::anyhow!("余额不足: {} < {}", balance, total_value));
                    }
                    
                    // 扣除报价资产，增加基础资产
                    *balances.entry(quote_asset.clone()).or_insert(Decimal::ZERO) -= total_value;
                    *balances.entry(base_asset.clone()).or_insert(Decimal::ZERO) += quantity;
                },
                Side::Sell => {
                    // 卖出需要检查基础资产余额
                    let balance = balances.get(&base_asset).cloned().unwrap_or_default();
                    if balance < quantity {
                        return Err(anyhow::anyhow!("余额不足: {} < {}", balance, quantity));
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
            status: crate::models::OrderStatus::Filled,  // 模拟环境中，订单立即成交
            timestamp: Utc::now(),
        };
        
        // 保存订单
        {
            let mut orders = self.orders.lock().unwrap();
            orders.insert(order_id, order.clone());
        }
        
        Ok(order)
    }
    
    async fn place_futures_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let (base_asset, quote_asset) = self.parse_symbol(symbol)?;
        
        // 获取当前价格
        let current_price = {
            let prices = self.prices.lock().unwrap();
            *prices.get(symbol).ok_or_else(|| anyhow::anyhow!("价格不可用: {}", symbol))?
        };
        
        // 使用指定价格或者当前市场价格
        let execution_price = price.unwrap_or(current_price);
        
        // 计算总价值
        let total_value = quantity * execution_price;
        
        // 检查合约账户余额
        {
            let mut balances = self.futures_balances.lock().unwrap();
            
            match side {
                Side::Buy => {
                    // 买入需要检查报价资产余额
                    let balance = balances.get(&quote_asset).cloned().unwrap_or_default();
                    if balance < total_value {
                        return Err(anyhow::anyhow!("合约账户余额不足: {} < {}", balance, total_value));
                    }
                    
                    // 扣除报价资产，增加基础资产
                    *balances.entry(quote_asset.clone()).or_insert(Decimal::ZERO) -= total_value;
                    *balances.entry(base_asset.clone()).or_insert(Decimal::ZERO) += quantity;
                },
                Side::Sell => {
                    // 卖出需要检查基础资产余额
                    let balance = balances.get(&base_asset).cloned().unwrap_or_default();
                    if balance < quantity {
                        return Err(anyhow::anyhow!("合约账户余额不足: {} < {}", balance, quantity));
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
            status: crate::models::OrderStatus::Filled,  // 模拟环境中，订单立即成交
            timestamp: Utc::now(),
        };
        
        // 保存订单
        {
            let mut orders = self.futures_orders.lock().unwrap();
            orders.insert(order_id, order.clone());
        }
        
        Ok(order)
    }
    
    async fn get_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let orders = self.orders.lock().unwrap();
        
        if let Some(order) = orders.get(&order_id) {
            if order.symbol == symbol {
                Ok(order.clone())
            } else {
                Err(anyhow::anyhow!("订单ID和交易对不匹配"))
            }
        } else {
            Err(anyhow::anyhow!("订单不存在: {}", order_id))
        }
    }
    
    async fn get_futures_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let orders = self.futures_orders.lock().unwrap();
        
        if let Some(order) = orders.get(&order_id) {
            if order.symbol == symbol {
                Ok(order.clone())
            } else {
                Err(anyhow::anyhow!("订单ID和交易对不匹配"))
            }
        } else {
            Err(anyhow::anyhow!("订单不存在: {}", order_id))
        }
    }
    
    async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let mut orders = self.orders.lock().unwrap();
        
        if let Some(mut order) = orders.get(&order_id).cloned() {
            if order.symbol == symbol {
                // 如果订单已经完成，则无法取消
                if order.status == crate::models::OrderStatus::Filled {
                    return Err(anyhow::anyhow!("无法取消已成交的订单"));
                }
                
                order.status = crate::models::OrderStatus::Cancelled;
                orders.insert(order_id, order.clone());
                
                Ok(order)
            } else {
                Err(anyhow::anyhow!("订单ID和交易对不匹配"))
            }
        } else {
            Err(anyhow::anyhow!("订单不存在: {}", order_id))
        }
    }
    
    async fn cancel_futures_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let mut orders = self.futures_orders.lock().unwrap();
        
        if let Some(mut order) = orders.get(&order_id).cloned() {
            if order.symbol == symbol {
                // 如果订单已经完成，则无法取消
                if order.status == crate::models::OrderStatus::Filled {
                    return Err(anyhow::anyhow!("无法取消已成交的订单"));
                }
                
                order.status = crate::models::OrderStatus::Cancelled;
                orders.insert(order_id, order.clone());
                
                Ok(order)
            } else {
                Err(anyhow::anyhow!("订单ID和交易对不匹配"))
            }
        } else {
            Err(anyhow::anyhow!("订单不存在: {}", order_id))
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
    
    async fn get_futures_account_balance(&self, asset: &str) -> Result<Decimal> {
        let balances = self.futures_balances.lock().unwrap();
        
        if let Some(balance) = balances.get(asset) {
            Ok(*balance)
        } else {
            Ok(Decimal::ZERO)  // 如果资产不存在，返回零余额
        }
    }
}