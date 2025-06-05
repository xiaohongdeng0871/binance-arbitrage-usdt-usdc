use crate::config::Config;
use crate::models::{OrderBook, Price, QuoteCurrency, Side, Symbol, OrderInfo, OrderStatus};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::{Utc, DateTime};
use hmac::{Hmac, Mac};
use reqwest::{Client, RequestBuilder, Url};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use log::{debug, info, warn, error};

type HmacSha256 = Hmac<Sha256>;

#[async_trait]
pub trait ExchangeApi {
    async fn get_symbol_info(&self, symbol: &str) -> Result<Symbol>;
    async fn get_price(&self, symbol: &str) -> Result<Price>;
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook>;
    async fn place_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo>;
    async fn get_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo>;
    async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo>;
    async fn get_account_balance(&self, asset: &str) -> Result<Decimal>;
}

pub struct BinanceApi {
    client: Client,
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
}

#[async_trait]
impl ExchangeApi for BinanceApi {
    async fn get_symbol_info(&self, symbol: &str) -> Result<Symbol> {
        let params: HashMap<String, String> = HashMap::new();
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
}
