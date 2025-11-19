use crate::config::Config;
use crate::models::{OrderBook, Price, Side, Symbol, OrderInfo, OrderStatus, FundingRate};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::{Utc};
use hmac::{Hmac, Mac};
use reqwest::{Client, RequestBuilder};
use rust_decimal::Decimal;
use sha2::Sha256;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// 交易所API接口，定义了与交易所交互所需的各种操作
#[async_trait]
pub trait ExchangeApi {
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

/// Binance交易所API实现
pub struct BinanceApi {
    /// HTTP客户端
    client: Client,
    /// 配置信息
    config: Config,
}

impl BinanceApi {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    fn get_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn sign_payload(&self, payload: &str) -> Result<String> {
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| anyhow!("Failed to create HMAC: {}", e))?;
        
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        let signature = result.into_bytes();
        
        Ok(hex::encode(signature))
    }

    async fn send_public_request(&self, endpoint: &str, params: Option<HashMap<String, String>>) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.config.base_url, endpoint);
        
        let mut request_builder = self.client.get(&url);
        
        if let Some(params) = params {
            request_builder = request_builder.query(&params);
        }
        
        self.send_request(request_builder).await
    }

    async fn send_signed_request(&self, endpoint: &str, method: &str, mut params: HashMap<String, String>) -> Result<serde_json::Value> {
        // 添加时间戳
        params.insert("timestamp".to_string(), self.get_timestamp().to_string());
        
        // 构建查询字符串
        let query = Self::build_query_string(&params);
        
        // 生成签名
        let signature = self.sign_payload(&query)?;
        params.insert("signature".to_string(), signature);
        
        let url = format!("{}{}", self.config.base_url, endpoint);
        
        let request_builder = match method {
            "GET" => self.client.get(&url).query(&params),
            "POST" => self.client.post(&url).query(&params),
            "DELETE" => self.client.delete(&url).query(&params),
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };
        
        let request_builder = request_builder.header("X-MBX-APIKEY", &self.config.api_key);
        
        self.send_request(request_builder).await
    }

    fn build_query_string(params: &HashMap<String, String>) -> String {
        let mut pairs: Vec<_> = params.iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        
        pairs.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&")
    }

    async fn send_request(&self, request_builder: RequestBuilder) -> Result<serde_json::Value> {
        let response = request_builder.send().await?;
        
        if response.status().is_success() {
            let json = response.json::<serde_json::Value>().await?;
            Ok(json)
        } else {
            let error_text = response.text().await?;
            Err(anyhow!("API error: {}", error_text))
        }
    }
    
    async fn send_futures_signed_request(&self, endpoint: &str, method: &str, mut params: HashMap<String, String>) -> Result<serde_json::Value> {
        // 添加时间戳
        params.insert("timestamp".to_string(), self.get_timestamp().to_string());
        
        // 构建查询字符串
        let query = Self::build_query_string(&params);
        
        // 生成签名
        let signature = self.sign_payload(&query)?;
        params.insert("signature".to_string(), signature);
        
        let url = format!("{}{}", "https://fapi.binance.com", endpoint); // 合约API地址
        
        let request_builder = match method {
            "GET" => self.client.get(&url).query(&params),
            "POST" => self.client.post(&url).query(&params),
            "DELETE" => self.client.delete(&url).query(&params),
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };
        
        let request_builder = request_builder.header("X-MBX-APIKEY", &self.config.api_key);
        
        self.send_request(request_builder).await
    }
}

#[async_trait]
impl ExchangeApi for BinanceApi {
    async fn get_symbol_info(&self, symbol: &str) -> Result<Symbol> {
        let _: HashMap<String, String> = HashMap::new();
        let response = self.send_public_request("/api/v3/exchangeInfo", None).await?;
        
        if let Some(symbols) = response["symbols"].as_array() {
            for sym in symbols {
                if sym["symbol"].as_str() == Some(symbol) {
                    let base_asset = sym["baseAsset"].as_str().unwrap_or_default().to_string();
                    let quote_asset = sym["quoteAsset"].as_str().unwrap_or_default().to_string();
                    
                    let mut min_notional = Decimal::ZERO;
                    let mut min_qty = Decimal::ZERO;
                    let mut step_size = Decimal::ZERO;
                    let mut tick_size = Decimal::ZERO;
                    
                    if let Some(filters) = sym["filters"].as_array() {
                        for filter in filters {
                            match filter["filterType"].as_str() {
                                Some("MIN_NOTIONAL") => {
                                    if let Some(val) = filter["minNotional"].as_str() {
                                        min_notional = val.parse::<Decimal>().unwrap_or_default();
                                    }
                                },
                                Some("LOT_SIZE") => {
                                    if let Some(val) = filter["minQty"].as_str() {
                                        min_qty = val.parse::<Decimal>().unwrap_or_default();
                                    }
                                    if let Some(val) = filter["stepSize"].as_str() {
                                        step_size = val.parse::<Decimal>().unwrap_or_default();
                                    }
                                },
                                Some("PRICE_FILTER") => {
                                    if let Some(val) = filter["tickSize"].as_str() {
                                        tick_size = val.parse::<Decimal>().unwrap_or_default();
                                    }
                                },
                                _ => {}
                            }
                        }
                    }
                    
                    return Ok(Symbol {
                        base_asset,
                        quote_asset,
                        min_notional,
                        min_qty,
                        step_size,
                        tick_size,
                    });
                }
            }
        }
        
        Err(anyhow!("Symbol not found: {}", symbol))
    }
    
    async fn get_price(&self, symbol: &str) -> Result<Price> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        
        let response = self.send_public_request("/api/v3/ticker/price", Some(params)).await?;
        
        let price_str = response["price"].as_str().context("Price not found in response")?;
        let price = price_str.parse::<Decimal>()?;
        
        Ok(Price {
            symbol: symbol.to_string(),
            price,
            timestamp: Utc::now(),
        })
    }
    
    async fn get_spot_price(&self, symbol: &str) -> Result<Price> {
        // 现货价格直接调用get_price
        self.get_price(symbol).await
    }
    
    async fn get_futures_price(&self, symbol: &str) -> Result<Price> {
        // 合约价格使用期货API
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        
        let response = self.send_public_request("/fapi/v1/ticker/price", Some(params)).await?;
        
        let price_str = response["price"].as_str().context("Price not found in response")?;
        let price = price_str.parse::<Decimal>()?;
        
        Ok(Price {
            symbol: symbol.to_string(),
            price,
            timestamp: Utc::now(),
        })
    }
    
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        
        if let Some(limit) = limit {
            params.insert("limit".to_string(), limit.to_string());
        }
        
        let response = self.send_public_request("/api/v3/depth", Some(params)).await?;
        
        let mut bids = Vec::new();
        if let Some(bid_array) = response["bids"].as_array() {
            for bid in bid_array {
                if let (Some(price_str), Some(qty_str)) = (bid[0].as_str(), bid[1].as_str()) {
                    let price = price_str.parse::<Decimal>()?;
                    let qty = qty_str.parse::<Decimal>()?;
                    bids.push((price, qty));
                }
            }
        }
        
        let mut asks = Vec::new();
        if let Some(ask_array) = response["asks"].as_array() {
            for ask in ask_array {
                if let (Some(price_str), Some(qty_str)) = (ask[0].as_str(), ask[1].as_str()) {
                    let price = price_str.parse::<Decimal>()?;
                    let qty = qty_str.parse::<Decimal>()?;
                    asks.push((price, qty));
                }
            }
        }
        
        Ok(OrderBook {
            symbol: symbol.to_string(),
            bids,
            asks,
            timestamp: Utc::now(),
        })
    }
    
    async fn get_funding_rate(&self, symbol: &str) -> Result<FundingRate> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        
        let response = self.send_public_request("/fapi/v1/fundingRate", Some(params)).await?;
        
        if let Some(rate_array) = response.as_array() {
            if let Some(latest_rate) = rate_array.first() {
                let funding_rate_str = latest_rate["fundingRate"].as_str().context("Funding rate not found in response")?;
                let funding_rate = funding_rate_str.parse::<Decimal>()?;
                
                Ok(FundingRate {
                    symbol: symbol.to_string(),
                    funding_rate,
                    timestamp: Utc::now(),
                })
            } else {
                Err(anyhow!("No funding rate data available for symbol: {}", symbol))
            }
        } else {
            Err(anyhow!("Invalid funding rate response format for symbol: {}", symbol))
        }
    }
    
    async fn place_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("side".to_string(), side.to_string());
        params.insert("quantity".to_string(), quantity.to_string());
        
        let order_type = if price.is_some() {
            "LIMIT"
        } else {
            "MARKET"
        };
        
        params.insert("type".to_string(), order_type.to_string());
        
        if let Some(price) = price {
            params.insert("price".to_string(), price.to_string());
            params.insert("timeInForce".to_string(), "GTC".to_string());
        }
        
        let response = self.send_signed_request("/api/v3/order", "POST", params).await?;
        
        let order_id = response["orderId"].as_u64().context("Order ID not found in response")?;
        let price = if let Some(p) = response["price"].as_str() {
            p.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        let qty = if let Some(q) = response["origQty"].as_str() {
            q.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        let status_str = response["status"].as_str().unwrap_or("NEW");
        let status = match status_str {
            "NEW" => OrderStatus::New,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED" => OrderStatus::Filled,
            "CANCELED" => OrderStatus::Cancelled,
            "REJECTED" => OrderStatus::Rejected,
            "EXPIRED" => OrderStatus::Expired,
            _ => OrderStatus::New,
        };
        
        Ok(OrderInfo {
            order_id,
            symbol: symbol.to_string(),
            price,
            qty,
            side,
            status,
            timestamp: Utc::now(),
        })
    }
    
    async fn place_futures_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("side".to_string(), side.to_string());
        params.insert("type".to_string(), "MARKET".to_string());
        
        if let Some(p) = price {
            params.insert("type".to_string(), "LIMIT".to_string());
            params.insert("price".to_string(), p.to_string());
            params.insert("timeInForce".to_string(), "GTC".to_string());
        }
        
        params.insert("quantity".to_string(), quantity.to_string());
        
        let response = self.send_futures_signed_request("/fapi/v1/order", "POST", params).await?;
        
        let order_id = response["orderId"].as_u64().context("Order ID not found in response")?;
        let price_str = response["price"].as_str().unwrap_or("0");
        let qty_str = response["origQty"].as_str().unwrap_or("0");
        let price = price_str.parse::<Decimal>()?;
        let qty = qty_str.parse::<Decimal>()?;
        let side = if response["side"].as_str() == Some("BUY") { Side::Buy } else { Side::Sell };
        
        let status = match response["status"].as_str() {
            Some("NEW") => OrderStatus::New,
            Some("FILLED") => OrderStatus::Filled,
            Some("PARTIALLY_FILLED") => OrderStatus::PartiallyFilled,
            Some("CANCELED") => OrderStatus::Cancelled,
            Some("REJECTED") => OrderStatus::Rejected,
            Some("EXPIRED") => OrderStatus::Expired,
            _ => OrderStatus::New,
        };
        
        Ok(OrderInfo {
            order_id,
            symbol: symbol.to_string(),
            price,
            qty,
            side,
            status,
            timestamp: Utc::now(),
        })
    }
    
    async fn get_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());
        
        let response = self.send_signed_request("/api/v3/order", "GET", params).await?;
        
        let side_str = response["side"].as_str().unwrap_or("BUY");
        let side = match side_str {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            _ => Side::Buy,
        };
        
        let price = if let Some(p) = response["price"].as_str() {
            p.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        let qty = if let Some(q) = response["origQty"].as_str() {
            q.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        let status_str = response["status"].as_str().unwrap_or("NEW");
        let status = match status_str {
            "NEW" => OrderStatus::New,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED" => OrderStatus::Filled,
            "CANCELED" => OrderStatus::Cancelled,
            "REJECTED" => OrderStatus::Rejected,
            "EXPIRED" => OrderStatus::Expired,
            _ => OrderStatus::New,
        };
        
        Ok(OrderInfo {
            order_id,
            symbol: symbol.to_string(),
            price,
            qty,
            side,
            status,
            timestamp: Utc::now(),
        })
    }
    
    async fn get_futures_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());
        
        let response = self.send_futures_signed_request("/fapi/v1/order", "GET", params).await?;
        
        let side_str = response["side"].as_str().unwrap_or("BUY");
        let side = match side_str {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            _ => Side::Buy,
        };
        
        let price = if let Some(p) = response["price"].as_str() {
            p.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        let qty = if let Some(q) = response["origQty"].as_str() {
            q.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        let status_str = response["status"].as_str().unwrap_or("NEW");
        let status = match status_str {
            "NEW" => OrderStatus::New,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED" => OrderStatus::Filled,
            "CANCELED" => OrderStatus::Cancelled,
            "REJECTED" => OrderStatus::Rejected,
            "EXPIRED" => OrderStatus::Expired,
            _ => OrderStatus::New,
        };
        
        Ok(OrderInfo {
            order_id,
            symbol: symbol.to_string(),
            price,
            qty,
            side,
            status,
            timestamp: Utc::now(),
        })
    }
    
    async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());
        
        let response = self.send_signed_request("/api/v3/order", "DELETE", params).await?;
        
        let side_str = response["side"].as_str().unwrap_or("BUY");
        let side = match side_str {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            _ => Side::Buy,
        };
        
        let price = if let Some(p) = response["price"].as_str() {
            p.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        let qty = if let Some(q) = response["origQty"].as_str() {
            q.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        Ok(OrderInfo {
            order_id,
            symbol: symbol.to_string(),
            price,
            qty,
            side,
            status: OrderStatus::Cancelled,
            timestamp: Utc::now(),
        })
    }
    
    async fn cancel_futures_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());
        
        let response = self.send_futures_signed_request("/fapi/v1/order", "DELETE", params).await?;
        
        let side_str = response["side"].as_str().unwrap_or("BUY");
        let side = match side_str {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            _ => Side::Buy,
        };
        
        let price = if let Some(p) = response["price"].as_str() {
            p.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        let qty = if let Some(q) = response["origQty"].as_str() {
            q.parse::<Decimal>()?
        } else {
            Decimal::ZERO
        };
        
        Ok(OrderInfo {
            order_id,
            symbol: symbol.to_string(),
            price,
            qty,
            side,
            status: OrderStatus::Cancelled,
            timestamp: Utc::now(),
        })
    }
    
    async fn get_account_balance(&self, asset: &str) -> Result<Decimal> {
        let params = HashMap::new();
        
        let response = self.send_signed_request("/api/v3/account", "GET", params).await?;
        
        if let Some(balances) = response["balances"].as_array() {
            for balance in balances {
                if balance["asset"].as_str() == Some(asset) {
                    let free = balance["free"].as_str().unwrap_or("0");
                    return Ok(free.parse::<Decimal>()?);
                }
            }
        }
        
        Err(anyhow!("Balance not found for asset: {}", asset))
    }
    
    async fn get_futures_account_balance(&self, asset: &str) -> Result<Decimal> {
        let params = HashMap::new();
        
        let response = self.send_futures_signed_request("/fapi/v2/account", "GET", params).await?;
        
        if let Some(balances) = response["assets"].as_array() {
            for balance in balances {
                if balance["asset"].as_str() == Some(asset) {
                    let free = balance["availableBalance"].as_str().unwrap_or("0");
                    return Ok(free.parse::<Decimal>()?);
                }
            }
        }
        
        Err(anyhow!("Balance not found for asset: {}", asset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::str::FromStr;

    #[test]
    fn test_build_query_string() {
        let mut params = HashMap::new();
        params.insert("timestamp".to_string(), "123456789".to_string());
        params.insert("symbol".to_string(), "BTCUSDT".to_string());
        params.insert("side".to_string(), "BUY".to_string());

        let query_string = BinanceApi::build_query_string(&params);
        // 由于HashMap是无序的，但函数内会排序，所以结果应该是按字母顺序排列的
        assert_eq!(query_string, "side=BUY&symbol=BTCUSDT&timestamp=123456789");
    }

    #[tokio::test]
    #[ignore] // 忽略此测试，因为它需要有效的API密钥
    async fn test_get_price() -> Result<()> {
        // 需要设置环境变量BINANCE_API_KEY和BINANCE_API_SECRET才能运行此测试
        let api_key = env::var("BINANCE_API_KEY").unwrap_or_default();
        let api_secret = env::var("BINANCE_API_SECRET").unwrap_or_default();
        
        if api_key.is_empty() || api_secret.is_empty() {
            println!("Skipping test_get_price: API keys not set");
            return Ok(());
        }

        let config = Config {
            api_key,
            api_secret,
            base_url: "https://api.binance.com".to_string(),
            arbitrage_settings: Default::default(),
            strategy_settings: Default::default(),
            risk_settings: Default::default(),
        };
        
        let api = BinanceApi::new(config);
        let price = api.get_price("BTCUSDT").await?;
        
        assert_eq!(price.symbol, "BTCUSDT");
        assert!(price.price > Decimal::ZERO);
        
        Ok(())
    }

    #[tokio::test]
    #[ignore] // 忽略此测试，因为它需要有效的API密钥
    async fn test_get_order_book() -> Result<()> {
        // 需要设置环境变量BINANCE_API_KEY和BINANCE_API_SECRET才能运行此测试
        let api_key = env::var("BINANCE_API_KEY").unwrap_or_default();
        let api_secret = env::var("BINANCE_API_SECRET").unwrap_or_default();
        
        if api_key.is_empty() || api_secret.is_empty() {
            println!("Skipping test_get_order_book: API keys not set");
            return Ok(());
        }

        let config = Config {
            api_key,
            api_secret,
            base_url: "https://api.binance.com".to_string(),
            arbitrage_settings: Default::default(),
            strategy_settings: Default::default(),
            risk_settings: Default::default(),
        };
        
        let api = BinanceApi::new(config);
        let order_book = api.get_order_book("BTCUSDT", Some(5)).await?;
        
        assert_eq!(order_book.symbol, "BTCUSDT");
        assert!(!order_book.bids.is_empty());
        assert!(!order_book.asks.is_empty());
        assert!(order_book.bids.len() <= 5);
        assert!(order_book.asks.len() <= 5);
        
        // 检查价格排序：买单价格应该从高到低排列
        for i in 1..order_book.bids.len() {
            assert!(order_book.bids[i-1].0 >= order_book.bids[i].0);
        }
        
        // 检查价格排序：卖单价格应该从低到高排列
        for i in 1..order_book.asks.len() {
            assert!(order_book.asks[i-1].0 <= order_book.asks[i].0);
        }
        
        Ok(())
    }

    #[test]
    fn test_sign_payload() -> Result<()> {
        let config = Config {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            base_url: "https://api.binance.com".to_string(),
            arbitrage_settings: Default::default(),
            strategy_settings: Default::default(),
            risk_settings: Default::default(),
        };

        let api = BinanceApi::new(config);
        let signature = api.sign_payload("symbol=BTCUSDT&side=BUY&timestamp=123456789")?;
        
        // 验证签名是一个十六进制字符串
        assert_eq!(signature.len(), 64); // SHA256哈希的十六进制表示是64个字符
        
        // 验证签名只包含十六进制字符
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
        
        Ok(())
    }

    #[tokio::test]
    #[ignore] // 忽略此测试，因为它需要有效的API密钥
    async fn test_get_account_balance() -> Result<()> {
        // 需要设置环境变量BINANCE_API_KEY和BINANCE_API_SECRET才能运行此测试
        let api_key = env::var("BINANCE_API_KEY").unwrap_or_default();
        let api_secret = env::var("BINANCE_API_SECRET").unwrap_or_default();
        
        if api_key.is_empty() || api_secret.is_empty() {
            println!("Skipping test_get_account_balance: API keys not set");
            return Ok(());
        }

        let config = Config {
            api_key,
            api_secret,
            base_url: "https://api.binance.com".to_string(),
            arbitrage_settings: Default::default(),
            strategy_settings: Default::default(),
            risk_settings: Default::default(),
        };
        
        let api = BinanceApi::new(config);
        let balance = api.get_account_balance("USDT").await?;
        
        assert!(balance >= Decimal::ZERO);
        
        Ok(())
    }

    #[tokio::test]
    #[ignore] // 忽略此测试，因为它需要有效的API密钥
    async fn test_place_and_cancel_order() -> Result<()> {
        // 需要设置环境变量BINANCE_API_KEY和BINANCE_API_SECRET才能运行此测试
        let api_key = env::var("BINANCE_API_KEY").unwrap_or_default();
        let api_secret = env::var("BINANCE_API_SECRET").unwrap_or_default();
        
        if api_key.is_empty() || api_secret.is_empty() {
            println!("Skipping test_place_and_cancel_order: API keys not set");
            return Ok(());
        }

        let config = Config {
            api_key,
            api_secret,
            base_url: "https://api.binance.com".to_string(),
            arbitrage_settings: Default::default(),
            strategy_settings: Default::default(),
            risk_settings: Default::default(),
        };
        
        let api = BinanceApi::new(config);
        
        // 先获取当前价格
        let price_info = api.get_price("BTCUSDT").await?;
        let price = price_info.price;
        
        // 下一个较低价格的限价单（确保不会立即成交）
        let lower_price = price * Decimal::from_str("0.95").unwrap(); 
        
        // 下单
        let order = api.place_order(
            "BTCUSDT", 
            Side::Buy, 
            Decimal::from_str("0.001").unwrap(), 
            Some(lower_price)
        ).await?;
        
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.qty, Decimal::from_str("0.001").unwrap());
        
        // 取消订单
        let cancelled_order = api.cancel_order("BTCUSDT", order.order_id).await?;
        assert_eq!(cancelled_order.order_id, order.order_id);
        // 注意：在真实环境中，订单状态可能是Cancelled，但在测试环境中可能仍是New
        
        Ok(())
    }
}