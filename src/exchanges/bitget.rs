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

/// Bitget交易所API实现
pub struct BitgetApi {
    /// HTTP客户端
    client: Client,
    /// 配置信息
    config: Config,
}

impl BitgetApi {
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

    fn sign_payload(&self, timestamp: u64, method: &str, endpoint: &str, body: &str) -> Result<String> {
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

    async fn send_signed_request(&self, endpoint: &str, method: &str, params: Option<HashMap<String, String>>, body: Option<String>) -> Result<serde_json::Value> {
        let timestamp = self.get_timestamp();
        let body_str = body.unwrap_or_default();
        
        let signature = self.sign_payload(timestamp, method, endpoint, &body_str)?;
        
        let url = format!("{}{}", self.config.base_url, endpoint);
        
        let request_builder = match method {
            "GET" => {
                if let Some(params) = params {
                    self.client.get(&url).query(&params)
                } else {
                    self.client.get(&url)
                }
            },
            "POST" => self.client.post(&url).body(body_str),
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };
        
        let request_builder = request_builder
            .header("ACCESS-KEY", &self.config.api_key)
            .header("ACCESS-SIGN", &signature)
            .header("ACCESS-TIMESTAMP", timestamp.to_string())
            .header("Content-Type", "application/json")
            .header("locale", "en-US");
        
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
impl ExchangeApi for BitgetApi {
    fn get_exchange(&self) -> Exchange {
        Exchange::Bitget
    }
    
    async fn get_symbol_info(&self, _symbol: &str) -> Result<Symbol> {
        // TODO: 实现Bitget的获取交易对信息接口
        Err(anyhow!("Not implemented"))
    }
    
    async fn get_price(&self, symbol: &str) -> Result<Price> {
        let endpoint = "/api/spot/v1/market/ticker";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        
        let response = self.send_public_request(endpoint, Some(params)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            let price_str = data["close"].as_str().context("Price not found in response")?;
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
        let endpoint = "/api/mix/v1/market/ticker";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("productType".to_string(), "UMCBL".to_string()); // UMCBL for USDT perpetual contracts
        
        let response = self.send_public_request(endpoint, Some(params)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            let price_str = data["last"].as_str().context("Price not found in response")?;
            let price = price_str.parse::<Decimal>()?;
            
            Ok(Price {
                symbol: symbol.to_string(),
                price,
                timestamp: Utc::now(),
            })
        } else {
            Err(anyhow!("Failed to get futures price: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook> {
        let endpoint = "/api/spot/v1/market/depth";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        
        if let Some(limit) = limit {
            params.insert("limit".to_string(), limit.to_string());
        }
        
        let response = self.send_public_request(endpoint, Some(params)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            
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
        // TODO: 实现Bitget的获取资金费率接口
        Err(anyhow!("Not implemented"))
    }
    
    async fn place_order(&self, symbol: &str, side: Side, quantity: Decimal, price: Option<Decimal>) -> Result<OrderInfo> {
        let endpoint = "/api/spot/v1/trade/orders";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("side".to_string(), side.to_string().to_uppercase());
        
        let order_type = if price.is_some() {
            "limit"
        } else {
            "market"
        };
        params.insert("orderType".to_string(), order_type.to_string());
        
        params.insert("force".to_string(), "gtc".to_string()); // Good till cancel
        params.insert("quantity".to_string(), quantity.to_string());
        
        if let Some(price) = price {
            params.insert("price".to_string(), price.to_string());
        }
        
        let body = serde_json::to_string(&params)?;
        let response = self.send_signed_request(endpoint, "POST", None, Some(body)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            let order_id_str = data["orderId"].as_str().context("Order ID not found in response")?;
            let order_id = order_id_str.parse::<u64>()?;
            
            let price = if let Some(p) = price {
                p
            } else {
                Decimal::ZERO
            };
            
            let status_str = data["status"].as_str().unwrap_or("new");
            let status = match status_str {
                "new" => OrderStatus::New,
                "partial-fill" => OrderStatus::PartiallyFilled,
                "full-fill" => OrderStatus::Filled,
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
        let endpoint = "/api/mix/v1/order/placeOrder";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("productType".to_string(), "UMCBL".to_string()); // UMCBL for USDT perpetual contracts
        params.insert("side".to_string(), side.to_string().to_uppercase());
        
        let order_type = if price.is_some() {
            "limit"
        } else {
            "market"
        };
        params.insert("orderType".to_string(), order_type.to_string());
        
        params.insert("force".to_string(), "gtc".to_string()); // Good till cancel
        params.insert("size".to_string(), quantity.to_string());
        
        if let Some(price) = price {
            params.insert("price".to_string(), price.to_string());
        }
        
        let body = serde_json::to_string(&params)?;
        let response = self.send_signed_request(endpoint, "POST", None, Some(body)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            let order_id_str = data["orderId"].as_str().context("Order ID not found in response")?;
            let order_id = order_id_str.parse::<u64>()?;
            
            let price = if let Some(p) = price {
                p
            } else {
                Decimal::ZERO
            };
            
            // Futures orders are initially new
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
            Err(anyhow!("Failed to place futures order: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_order_status(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let endpoint = "/api/spot/v1/trade/orderInfo";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());
        
        let body = serde_json::to_string(&params)?;
        let response = self.send_signed_request(endpoint, "POST", None, Some(body)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"][0];
            let side_str = data["side"].as_str().unwrap_or("buy");
            let side = match side_str.to_lowercase().as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => Side::Buy,
            };
            
            let price_str = data["price"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = data["quantity"].as_str().unwrap_or("0");
            let qty = qty_str.parse::<Decimal>()?;
            
            let status_str = data["status"].as_str().unwrap_or("new");
            let status = match status_str {
                "new" => OrderStatus::New,
                "partial-fill" => OrderStatus::PartiallyFilled,
                "full-fill" => OrderStatus::Filled,
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
        let endpoint = "/api/mix/v1/order/detail";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("productType".to_string(), "UMCBL".to_string()); // UMCBL for USDT perpetual contracts
        params.insert("orderId".to_string(), order_id.to_string());
        
        let body = serde_json::to_string(&params)?;
        let response = self.send_signed_request(endpoint, "POST", None, Some(body)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            let side_str = data["side"].as_str().unwrap_or("buy");
            let side = match side_str.to_lowercase().as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => Side::Buy,
            };
            
            let price_str = data["price"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = data["size"].as_str().unwrap_or("0");
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
            Err(anyhow!("Failed to get futures order status: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn cancel_order(&self, symbol: &str, order_id: u64) -> Result<OrderInfo> {
        let endpoint = "/api/spot/v1/trade/cancel-order";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());
        
        let body = serde_json::to_string(&params)?;
        let response = self.send_signed_request(endpoint, "POST", None, Some(body)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            let side_str = data["side"].as_str().unwrap_or("buy");
            let side = match side_str.to_lowercase().as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => Side::Buy,
            };
            
            let price_str = data["price"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = data["quantity"].as_str().unwrap_or("0");
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
        let endpoint = "/api/mix/v1/order/cancel-order";
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("productType".to_string(), "UMCBL".to_string()); // UMCBL for USDT perpetual contracts
        params.insert("orderId".to_string(), order_id.to_string());
        
        let body = serde_json::to_string(&params)?;
        let response = self.send_signed_request(endpoint, "POST", None, Some(body)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            let side_str = data["side"].as_str().unwrap_or("buy");
            let side = match side_str.to_lowercase().as_str() {
                "buy" => Side::Buy,
                "sell" => Side::Sell,
                _ => Side::Buy,
            };
            
            let price_str = data["price"].as_str().unwrap_or("0");
            let price = price_str.parse::<Decimal>()?;
            
            let qty_str = data["size"].as_str().unwrap_or("0");
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
            Err(anyhow!("Failed to cancel futures order: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_account_balance(&self, asset: &str) -> Result<Decimal> {
        let endpoint = "/api/spot/v1/account/assets";
        let response = self.send_signed_request(endpoint, "GET", None, None).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            if let Some(assets) = data.as_array() {
                for asset_data in assets {
                    if asset_data["coinName"].as_str() == Some(asset) {
                        let free_str = asset_data["available"].as_str().unwrap_or("0");
                        return Ok(free_str.parse::<Decimal>()?);
                    }
                }
            }
            Err(anyhow!("Balance not found for asset: {}", asset))
        } else {
            Err(anyhow!("Failed to get account balance: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
    
    async fn get_futures_account_balance(&self, _asset: &str) -> Result<Decimal> {
        let endpoint = "/api/mix/v1/account/accounts";
        let mut params = HashMap::new();
        params.insert("productType".to_string(), "UMCBL".to_string()); // UMCBL for USDT perpetual contracts
        
        let body = serde_json::to_string(&params)?;
        let response = self.send_signed_request(endpoint, "POST", None, Some(body)).await?;
        
        if response["code"].as_str() == Some("00000") {
            let data = &response["data"];
            let equity_str = data["equity"].as_str().unwrap_or("0");
            let equity = equity_str.parse::<Decimal>()?;
            Ok(equity)
        } else {
            Err(anyhow!("Failed to get futures account balance: {}", response["msg"].as_str().unwrap_or("Unknown error")))
        }
    }
}