use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use chrono::{DateTime, Utc};

/// 交易对类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuoteCurrency {
    USDT,
    USDC,
}

impl fmt::Display for QuoteCurrency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuoteCurrency::USDT => write!(f, "USDT"),
            QuoteCurrency::USDC => write!(f, "USDC"),
        }
    }
}

/// 交易对信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub base_asset: String,      // 基础资产，如 BTC
    pub quote_asset: String,     // 报价资产，如 USDT
    pub min_notional: Decimal,   // 最小交易金额
    pub min_qty: Decimal,        // 最小交易量
    pub step_size: Decimal,      // 数量精度
    pub tick_size: Decimal,      // 价格精度
}

/// 市场价格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub symbol: String,
    pub price: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 订单簿快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<(Decimal, Decimal)>,  // (价格, 数量)
    pub asks: Vec<(Decimal, Decimal)>,  // (价格, 数量)
    pub timestamp: DateTime<Utc>,
}

/// 订单方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
}

/// 套利机会
#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub base_asset: String,                 // 基础资产 如 BTC
    pub buy_quote: QuoteCurrency,           // 买入的报价货币 (USDT/USDC)
    pub sell_quote: QuoteCurrency,          // 卖出的报价货币 (USDT/USDC)
    pub buy_price: Decimal,                 // 买入价格
    pub sell_price: Decimal,                // 卖出价格
    pub price_diff: Decimal,                // 价格差异
    pub profit_percentage: Decimal,         // 利润百分比
    pub max_trade_amount: Decimal,          // 最大交易量
    pub timestamp: DateTime<Utc>,           // 时间戳
}

impl ArbitrageOpportunity {
    pub fn new(
        base_asset: &str,
        buy_quote: QuoteCurrency,
        sell_quote: QuoteCurrency,
        buy_price: Decimal,
        sell_price: Decimal,
        max_trade_amount: Decimal,
    ) -> Self {
        let price_diff = sell_price - buy_price;
        let profit_percentage = if buy_price.is_zero() {
            Decimal::ZERO
        } else {
            (price_diff / buy_price) * Decimal::from(100)
        };

        Self {
            base_asset: base_asset.to_string(),
            buy_quote,
            sell_quote,
            buy_price,
            sell_price,
            price_diff,
            profit_percentage,
            max_trade_amount,
            timestamp: Utc::now(),
        }
    }
}

/// 订单信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    pub order_id: u64,
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub side: Side,
    pub status: OrderStatus,
    pub timestamp: DateTime<Utc>,
}

/// 订单状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

/// 套利结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageResult {
    pub base_asset: String,
    pub buy_quote: String,
    pub sell_quote: String,
    pub buy_price: Decimal,
    pub sell_price: Decimal,
    pub trade_amount: Decimal,
    pub profit: Decimal,
    pub profit_percentage: Decimal,
    pub buy_order_id: Option<u64>,
    pub sell_order_id: Option<u64>,
    pub status: ArbitrageStatus,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
}

/// 套利状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArbitrageStatus {
    Identified,
    BuyOrderPlaced,
    BuyOrderFilled,
    SellOrderPlaced,
    SellOrderFilled,
    Completed,
    Failed,
}
