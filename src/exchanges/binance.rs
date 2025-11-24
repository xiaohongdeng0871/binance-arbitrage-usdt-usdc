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
use std::sync::{Arc, Mutex};
use log::{info};
use rust_decimal::dec;

type HmacSha256 = Hmac<Sha256>;

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
    fn get_exchange(&self) -> Exchange {
        Exchange::Binance
    }
    
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
    use crate::exchanges::exchange::MockExchangeApi;
    use rust_decimal::dec;

    #[tokio::test]
    async fn test_mock_api() {
        let api = MockExchangeApi::new(Exchange::Binance);
        
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