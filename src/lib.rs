//! # 币安 USDT/USDC 套利库
//!
//! 这个库提供了在币安交易所上进行USDT和USDC交易对之间套利的功能。
//! 它可以监控价格差异，并在满足设定的利润阈值时执行交易。
//! 
//! ## 主要组件
//! 
//! - `Config`: 配置结构体，用于存储API密钥和套利参数
//! - `ArbitrageEngine`: 套利引擎，实现套利逻辑
//! - `BinanceApi`: 币安API客户端，用于与币安交易所通信
//! - `MockBinanceApi`: 模拟API客户端，用于测试和开发
//! - `DatabaseManager`: 数据库管理器，用于存储和检索套利历史记录
//! - `AnalyticsManager`: 分析管理器，用于生成套利绩效报告和统计数据

pub mod arbitrage;
pub mod binance;
pub mod config;
pub mod models;
pub mod strategies;
pub mod risk;
pub mod db;
pub mod analytics;

// 重导出主要类型
pub use arbitrage::ArbitrageEngine;
pub use binance::{BinanceApi, ExchangeApi, MockBinanceApi};
pub use config::Config;
pub use models::{
    ArbitrageOpportunity, ArbitrageResult, ArbitrageStatus, 
    OrderBook, OrderInfo, OrderStatus, Price, QuoteCurrency, Side, Symbol,
};
pub use db::{DatabaseManager, TradeStats, DailyStats, AssetStats};
pub use analytics::{AnalyticsManager, PerformanceReport, TimeRange};
