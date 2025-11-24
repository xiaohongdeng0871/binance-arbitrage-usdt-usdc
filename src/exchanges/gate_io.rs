use crate::config::Config;
use crate::exchanges::exchange::{ExchangeApi, Exchange};
use crate::models::{OrderBook, Price, Side, Symbol, OrderInfo, OrderStatus, FundingRate};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::{Utc};
use hmac::{Hmac, Mac};
use reqwest::{Client, RequestBuilder};
use rust_decimal::Decimal;
use sha2::Sha512;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use base64::engine::general_purpose;

type HmacSha512 = Hmac<Sha512>;

/// Gate.io交易所API实现
pub struct GateIoApi {
    /// HTTP客户端
    client: Client,
    /// 配置信息
    config: Config,
}

impl GateIoApi {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    fn get_timestamp(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        
        format!("{:.3}", timestamp)
    }

    fn sign_payload(&self, method: &str, endpoint: &str, query_string: &str, body: &str, timestamp: &str) -> Result<String> {
        let prehash = format!("{}{}{}{}{}", timestamp, method, endpoint, query_string, body);
        let mut mac = HmacSha512::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| anyhow!("Failed to create HMAC: {}", e))?;
        
        mac.update(prehash.as_bytes());
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

    async fn send_signed_request(&self, endpoint: &str, method: &str, params: Option<HashMap<String, String>>, body: Option<String>) -> Result<serde_json::Value> {
        let timestamp = self.get_timestamp();
        let query_string = if let Some(ref p) = params {
            let mut pairs: Vec<_> = p.iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(b.0));
            pairs.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&")
        } else {
            "".to_string()
        };
        
        let body_str = body.unwrap_or_default();
        let signature = self.sign_payload(method, endpoint, &query_string, &body_str, &timestamp)?;
        
        let url = if query_string.is_empty() {
            format!("{}{}", self.config.base_url, endpoint)
        } else {
            format!("{}{}?{}", self.config.base_url, endpoint, query_string)
        };
        
        let request_builder = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url).body(body_str),
            "DELETE" => self.client.delete(&url),
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };
        
        let request_builder = request_builder
            .header("KEY", &self.config.api_key)
            .header("SIGN", &signature)
            .header("Timestamp", &timestamp)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");
        
        self.send_request(request_builder).await
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
impl ExchangeApi for GateIoApi {
    fn get_exchange(&self) -> Exchange {
        Exchange::GateIo
    }
    
    async fn get_symbol_info(&self, _symbol: &str) -> Result<Symbol> {
        // TODO: 实现Gate.io的获取交易对信息接口
        Err(anyhow!("Not implemented"))
    }
    
    async fn get_price(&self, symbol: &str) -> Result<Price> {
        let endpoint = format!("/api/v4/spot/tickers");
        let mut params = HashMap::new();
        params.insert("currency_pair".to_string(), symbol.to_string());
        
        let response = self.send_public_request(&endpoint, Some(params)).await?;
        
        if let Some(tickers) = response.as_array() {
            if let Some(ticker) = tickers.first() {
                let price_str = ticker["last"].as_str().context("Price not found in response")?;
                let price = price_str.parse::<Decimal>()?;
                
                Ok(Price {
                    symbol: symbol.to_string(),
                    price,
                    timestamp: Utc::now(),
                })
            } else {
                Err(anyhow!("No ticker data found for symbol: {}", symbol))
            }
        } else {
            Err(anyhow!("Invalid response format for symbol: {}", symbol))
        }
    }
    
    async fn get_spot_price(&self, symbol: &str) -> Result<Price> {
        self.get_price(symbol).await
    }
    
    async fn get_futures_price(&self, symbol: &str) -> Result<Price> {
        let endpoint = format!("/api/v4/futures/usdt/tickers");
        let mut params = HashMap::new();
        params.insert("contract".to_string(), symbol.to_string());
        
        let response = self.send_public_request(&endpoint, Some(params)).await?;
        
        if let Some(tickers) = response.as_array() {
            if let Some(ticker) = tickers.first() {
                let price_str = ticker["last"].as_str().context("Price not found in response")?;
                let price = price_str.parse::<Decimal>()?;
                
                Ok(Price {
                    symbol: symbol.to_string(),
                    price,
                    timestamp: Utc::now(),
                })
            } else {
                Err(anyhow!("No ticker data found for symbol: {}", symbol))
            }
        } else {
            Err(anyhow!("Invalid response format for symbol: {}", symbol))
        }
    }
    
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook> {
        let endpoint = format!("/api/v4/spot/order_book");
        let mut params = HashMap::new();
        params.insert("currency_pair".to_string(), symbol.to_string());
        
        if let Some(limit) = limit {
            params.insert("limit".to_string(), limit.to_string());
        }
        
        let response = self.send_public_request(&endpoint, Some(params)).await?;
        
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
    
    async fn get_funding_rate(&self, _symbol: &str) -> Result<FundingRate> {
        // TODO: 实现Gate.io的获取资金费率接口
        Err(anyhow!("Not implemented"))
    }
    
    async fn place_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let endpoint = "/api/v4/spot/orders";
        let mut params = HashMap::new();
        params.insert("currency_pair".to_string(), symbol.to_string());
        params.insert("side".to_string(), side.to_string().to_lowercase());
        
        let order_type = if price.is_some() {
            "limit"
        } else {
            "market"
        };
        params.insert("type".to_string(), order_type.to_string());
        
        params.insert("amount".to_string(), quantity.to_string());
        
        if let Some(price) = price {
            params.insert("price".to_string(), price.to_string());
        }
        
        let response = self.send_signed_request(&endpoint, "POST", Some(params), None).await?;
        
        if response["id"].is_string() {
            let order_id_str = response["id"].as_str().context("Order ID not found in response")?;
            let order_id = order_id_str.parse::<u64>()?;
            
            let price = if let Some(p) = price {
                p
            } else {
                Decimal::ZERO
            };
            
            let status_str = response["status"].as_str().unwrap_or("open");
            let status = match status_str {
                "open" => OrderStatus::New,
                "filled" => OrderStatus::Filled,
                "cancelled" => OrderStatus::Cancelled,
                _ => OrderStatus::New,
            };
            
            Ok(OrderInfo {
                order_id,
                symbol: symbol.to_string(),
                price,
                qty: quantity,
                side,
                status,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow!("Failed to place order: {}", response["label"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn place_futures_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let endpoint = "/api/v4/futures/usdt/orders";
        let mut params = HashMap::new();
        params.insert("contract".to_string(), symbol.to_string());
        params.insert("size".to_string(), quantity.to_string());
        
        if side == Side::Buy {
            params.insert("side".to_string(), "1".to_string()); // 1 for buy
        } else {
            params.insert("side".to_string(), "-1".to_string()); // -1 for sell
        }
        
        if let Some(price) = price {
            params.insert("price".to_string(), price.to_string());
            params.insert("tif".to_string(), "gtc".to_string()); // Good till cancel
        } else {
            params.insert("tif".to_string(), "ioc".to_string()); // Immediate or cancel for market orders
        }
        
        let response = self.send_signed_request(&endpoint, "POST", Some(params), None).await?;
        
        if response["id"].is_number() {
            let order_id = response["id"].as_u64().context("Order ID not found in response")?;
            
            let price = if let Some(p) = price {
                p
            } else {
                Decimal::ZERO
            };
            
            // Futures orders are initially open
            let status = OrderStatus::New;
            
            Ok(OrderInfo {
                order_id,
                symbol: symbol.to_string(),
                price,
                qty: quantity,
                side,
                status,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow!("Failed to place futures order: {}", response["label"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let endpoint = format!("/api/v4/spot/orders/{}", order_id);
        let mut params = HashMap::new();
        params.insert("currency_pair".to_string(), symbol.to_string());
        
        let response = self.send_signed_request(&endpoint, "GET", Some(params), None).await?;
        
        if response["id"].is_string() {
            let side_str = response["side"].as_str().unwrap_or("buy");
            let side = match side_str {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => Side::Buy,
            };
            
            let price_str = response["price"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = response["amount"].as_str().unwrap_or("0");
            let qty = qty_str.parse::<Decimal>()?;
            
            let status_str = response["status"].as_str().unwrap_or("open");
            let status = match status_str {
                "open" => OrderStatus::New,
                "filled" => OrderStatus::Filled,
                "cancelled" => OrderStatus::Cancelled,
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
        } else {
            Err(anyhow!("Failed to get order status: {}", response["label"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_futures_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let endpoint = format!("/api/v4/futures/usdt/orders/{}", order_id);
        let mut params = HashMap::new();
        params.insert("contract".to_string(), symbol.to_string());
        
        let response = self.send_signed_request(&endpoint, "GET", Some(params), None).await?;
        
        if response["id"].is_number() {
            // Gate.io futures uses 1 for buy and -1 for sell
            let side_value = response["side"].as_i64().unwrap_or(1);
            let side = if side_value > 0 { Side::Buy } else { Side::Sell };
            
            let price_str = response["price"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = response["size"].as_str().unwrap_or("0");
            let qty = qty_str.parse::<Decimal>()?;
            
            // Futures order status mapping
            let status = OrderStatus::New; // Simplified for now
            
            Ok(OrderInfo {
                order_id,
                symbol: symbol.to_string(),
                price,
                qty,
                side,
                status,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow!("Failed to get futures order status: {}", response["label"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let endpoint = format!("/api/v4/spot/orders/{}", order_id);
        let mut params = HashMap::new();
        params.insert("currency_pair".to_string(), symbol.to_string());
        
        let response = self.send_signed_request(&endpoint, "DELETE", Some(params), None).await?;
        
        if response["id"].is_string() {
            let side_str = response["side"].as_str().unwrap_or("buy");
            let side = match side_str {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => Side::Buy,
            };
            
            let price_str = response["price"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = response["amount"].as_str().unwrap_or("0");
            let qty = qty_str.parse::<Decimal>()?;
            
            Ok(OrderInfo {
                order_id,
                symbol: symbol.to_string(),
                price,
                qty,
                side,
                status: OrderStatus::Cancelled,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow!("Failed to cancel order: {}", response["label"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn cancel_futures_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let endpoint = format!("/api/v4/futures/usdt/orders/{}", order_id);
        let mut params = HashMap::new();
        params.insert("contract".to_string(), symbol.to_string());
        
        let response = self.send_signed_request(&endpoint, "DELETE", Some(params), None).await?;
        
        if response["id"].is_number() {
            // Gate.io futures uses 1 for buy and -1 for sell
            let side_value = response["side"].as_i64().unwrap_or(1);
            let side = if side_value > 0 { Side::Buy } else { Side::Sell };
            
            let price_str = response["price"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = response["size"].as_str().unwrap_or("0");
            let qty = qty_str.parse::<Decimal>()?;
            
            Ok(OrderInfo {
                order_id,
                symbol: symbol.to_string(),
                price,
                qty,
                side,
                status: OrderStatus::Cancelled,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow!("Failed to cancel futures order: {}", response["label"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_account_balance(&self, asset: &str) -> Result<Decimal> {
        let endpoint = "/api/v4/spot/accounts";
        let response = self.send_signed_request(&endpoint, "GET", None, None).await?;
        
        if let Some(accounts) = response.as_array() {
            for account in accounts {
                if account["currency"].as_str() == Some(asset) {
                    let free_str = account["available"].as_str().unwrap_or("0");
                    return Ok(free_str.parse::<Decimal>()?);
                }
            }
            Err(anyhow!("Balance not found for asset: {}", asset))
        } else {
            Err(anyhow!("Invalid response format when getting account balance"))
        }
    }
    
    async fn get_futures_account_balance(&self, _asset: &str) -> Result<Decimal> {
        let endpoint = "/api/v4/futures/usdt/accounts";
        let response = self.send_signed_request(&endpoint, "GET", None, None).await?;
        
        if response["available"].is_string() {
            let free_str = response["available"].as_str().unwrap_or("0");
            let free = free_str.parse::<Decimal>()?;
            Ok(free)
        } else {
            Err(anyhow!("Failed to get futures account balance: {}", response["label"].as_str().unwrap_or("Unknown error")))
        }
    }
}