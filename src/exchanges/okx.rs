use crate::config::Config;
use crate::exchanges::exchange::{ExchangeApi, Exchange};
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
use base64::{Engine as _, engine::general_purpose};

type HmacSha256 = Hmac<Sha256>;

/// OKX交易所API实现
pub struct OkxApi {
    /// HTTP客户端
    client: Client,
    /// 配置信息
    config: Config,
}

impl OkxApi {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    fn get_timestamp(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        
        // OKX使用ISO时间格式
        let seconds = now / 1000;
        let millis = now % 1000;
        format!("{}.{:03}Z", seconds, millis)
    }

    fn sign_payload(&self, timestamp: &str, method: &str, endpoint: &str, body: &str) -> Result<String> {
        let prehash = format!("{}{}{}{}", timestamp, method, endpoint, body);
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| anyhow!("Failed to create HMAC: {}", e))?;
        
        mac.update(prehash.as_bytes());
        let result = mac.finalize();
        let signature = result.into_bytes();
        
        Ok(general_purpose::STANDARD.encode(signature))
    }

    async fn send_public_request(&self, endpoint: &str, params: Option<HashMap<String, String>>) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.config.base_url, endpoint);
        
        let mut request_builder = self.client.get(&url);
        
        if let Some(params) = params {
            request_builder = request_builder.query(&params);
        }
        
        self.send_request(request_builder).await
    }

    async fn send_signed_request(&self, endpoint: &str, method: &str, params: Option<HashMap<String, String>>) -> Result<serde_json::Value> {
        let timestamp = self.get_timestamp();
        let body = if let Some(ref p) = params {
            serde_json::to_string(p)?
        } else {
            "".to_string()
        };
        
        let signature = self.sign_payload(&timestamp, method, endpoint, &body)?;
        
        let url = format!("{}{}", self.config.base_url, endpoint);
        
        let request_builder = match method {
            "GET" => self.client.get(&url).query(&params),
            "POST" => self.client.post(&url).body(body),
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };
        
        let request_builder = request_builder
            .header("OK-ACCESS-KEY", &self.config.api_key)
            .header("OK-ACCESS-SIGN", &signature)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", "") // 需要从配置中获取
            .header("Content-Type", "application/json");
        
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
impl ExchangeApi for OkxApi {
    fn get_exchange(&self) -> Exchange {
        Exchange::Okx
    }
    
    async fn get_symbol_info(&self, _symbol: &str) -> Result<Symbol> {
        // TODO: 实现OKX的获取交易对信息接口
        Err(anyhow!("Not implemented"))
    }
    
    async fn get_price(&self, symbol: &str) -> Result<Price> {
        let endpoint = "/api/v5/market/ticker";
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());
        
        let response = self.send_public_request(endpoint, Some(params)).await?;
        
        if response["code"].as_str() == Some("0") {
            let data = &response["data"][0];
            let price_str = data["last"].as_str().context("Price not found in response")?;
            let price = price_str.parse::<Decimal>()?;
            
            Ok(Price {
                symbol: symbol.to_string(),
                price,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow!("Failed to get price: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_spot_price(&self, symbol: &str) -> Result<Price> {
        self.get_price(symbol).await
    }
    
    async fn get_futures_price(&self, symbol: &str) -> Result<Price> {
        self.get_price(symbol).await
    }
    
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook> {
        let endpoint = "/api/v5/market/books";
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());
        
        if let Some(limit) = limit {
            params.insert("sz".to_string(), limit.to_string());
        }
        
        let response = self.send_public_request(endpoint, Some(params)).await?;
        
        if response["code"].as_str() == Some("0") {
            let data = &response["data"][0];
            
            let mut bids = Vec::new();
            if let Some(bid_array) = data["bids"].as_array() {
                for bid in bid_array {
                    if let (Some(price_str), Some(qty_str)) = (bid[0].as_str(), bid[1].as_str()) {
                        let price = price_str.parse::<Decimal>()?;
                        let qty = qty_str.parse::<Decimal>()?;
                        bids.push((price, qty));
                    }
                }
            }
            
            let mut asks = Vec::new();
            if let Some(ask_array) = data["asks"].as_array() {
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
        } else {
            Err(anyhow!("Failed to get order book: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_funding_rate(&self, _symbol: &str) -> Result<FundingRate> {
        // TODO: 实现OKX的获取资金费率接口
        Err(anyhow!("Not implemented"))
    }
    
    async fn place_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let endpoint = "/api/v5/trade/order";
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());
        params.insert("tdMode".to_string(), "cash".to_string()); // 现货交易
        params.insert("side".to_string(), side.to_string().to_lowercase());
        
        let order_type = if price.is_some() {
            "limit"
        } else {
            "market"
        };
        params.insert("ordType".to_string(), order_type.to_string());
        
        params.insert("sz".to_string(), quantity.to_string());
        
        if let Some(price) = price {
            params.insert("px".to_string(), price.to_string());
        }
        
        let response = self.send_signed_request(endpoint, "POST", Some(params)).await?;
        
        if response["code"].as_str() == Some("0") {
            let data = &response["data"][0];
            let order_id_str = data["ordId"].as_str().context("Order ID not found in response")?;
            let order_id = order_id_str.parse::<u64>()?;
            
            let price = if let Some(p) = price {
                p
            } else {
                Decimal::ZERO
            };
            
            let status_str = data["state"].as_str().unwrap_or("live");
            let status = match status_str {
                "live" => OrderStatus::New,
                "partially_filled" => OrderStatus::PartiallyFilled,
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
            Err(anyhow!("Failed to place order: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn place_futures_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let endpoint = "/api/v5/trade/order";
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());
        params.insert("tdMode".to_string(), "cross".to_string()); // 全仓交易
        params.insert("side".to_string(), side.to_string().to_lowercase());
        
        let order_type = if price.is_some() {
            "limit"
        } else {
            "market"
        };
        params.insert("ordType".to_string(), order_type.to_string());
        
        params.insert("sz".to_string(), quantity.to_string());
        
        if let Some(price) = price {
            params.insert("px".to_string(), price.to_string());
        }
        
        let response = self.send_signed_request(endpoint, "POST", Some(params)).await?;
        
        if response["code"].as_str() == Some("0") {
            let data = &response["data"][0];
            let order_id_str = data["ordId"].as_str().context("Order ID not found in response")?;
            let order_id = order_id_str.parse::<u64>()?;
            
            let price = if let Some(p) = price {
                p
            } else {
                Decimal::ZERO
            };
            
            let status_str = data["state"].as_str().unwrap_or("live");
            let status = match status_str {
                "live" => OrderStatus::New,
                "partially_filled" => OrderStatus::PartiallyFilled,
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
            Err(anyhow!("Failed to place futures order: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let endpoint = "/api/v5/trade/order";
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());
        params.insert("ordId".to_string(), order_id.to_string());
        
        let response = self.send_signed_request(endpoint, "GET", Some(params)).await?;
        
        if response["code"].as_str() == Some("0") {
            let data = &response["data"][0];
            let side_str = data["side"].as_str().unwrap_or("buy");
            let side = match side_str {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => Side::Buy,
            };
            
            let price_str = data["px"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = data["sz"].as_str().unwrap_or("0");
            let qty = qty_str.parse::<Decimal>()?;
            
            let status_str = data["state"].as_str().unwrap_or("live");
            let status = match status_str {
                "live" => OrderStatus::New,
                "partially_filled" => OrderStatus::PartiallyFilled,
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
            Err(anyhow!("Failed to get order status: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_futures_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        self.get_order_status(symbol, order_id).await
    }
    
    async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let endpoint = "/api/v5/trade/cancel-order";
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());
        params.insert("ordId".to_string(), order_id.to_string());
        
        let response = self.send_signed_request(endpoint, "POST", Some(params)).await?;
        
        if response["code"].as_str() == Some("0") {
            let data = &response["data"][0];
            let side_str = data["side"].as_str().unwrap_or("buy");
            let side = match side_str {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => Side::Buy,
            };
            
            let price_str = data["px"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = data["sz"].as_str().unwrap_or("0");
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
            Err(anyhow!("Failed to cancel order: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn cancel_futures_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        self.cancel_order(symbol, order_id).await
    }
    
    async fn get_account_balance(&self, asset: &str) -> Result<Decimal> {
        let endpoint = "/api/v5/account/balance";
        let response = self.send_signed_request(endpoint, "GET", None).await?;
        
        if response["code"].as_str() == Some("0") {
            let data = &response["data"][0];
            if let Some(details) = data["details"].as_array() {
                for detail in details {
                    if detail["ccy"].as_str() == Some(asset) {
                        let free_str = detail["availBal"].as_str().unwrap_or("0");
                        return Ok(free_str.parse::<Decimal>()?);
                    }
                }
            }
            Err(anyhow!("Balance not found for asset: {}", asset))
        } else {
            Err(anyhow!("Failed to get account balance: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_futures_account_balance(&self, asset: &str) -> Result<Decimal> {
        // 在OKX中，期货和现货使用相同的账户体系
        self.get_account_balance(asset).await
    }
}