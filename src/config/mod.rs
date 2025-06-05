use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::Read;
use anyhow::{Context, Result};
use rust_decimal::Decimal;

/// 交易策略类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StrategyType {
    /// 简单价格差异套利
    Simple,
    /// 时间加权平均价格(TWAP)
    TimeWeighted,
    /// 订单簿深度分析
    OrderBookDepth,
    /// 滑点控制
    SlippageControl,
    /// 趋势跟踪
    TrendFollowing,
}

/// 风控组件类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskControllerType {
    /// 每日亏损限制
    DailyLossLimit,
    /// 异常价格保护
    AbnormalPrice,
    /// 风险敞口控制
    Exposure,
    /// 交易时间窗口
    TradingTimeWindow,
    /// 交易频率控制
    TradingFrequency,
    /// 交易对黑名单
    PairBlacklist,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub api_key: String,
    pub api_secret: String,
    pub base_url: String,
    pub arbitrage_settings: ArbitrageSettings,
    pub strategy_settings: StrategySettings,
    pub risk_settings: RiskSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArbitrageSettings {
    pub min_profit_percentage: f64,   // 0.1%最小利润率
    pub max_trade_amount_usdt: f64, // 最大交易金额，USDT
    pub price_diff_threshold: f64,   // 价格差异阈值，百分比
    pub usdt_symbol: String,
    pub usdc_symbol: String,
    pub check_interval_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategySettings {
    /// 启用的交易策略列表
    pub enabled_strategies: Vec<StrategyType>,
    
    /// 时间加权平均价格策略设置
    pub twap: TwapStrategySettings,
    
    /// 订单簿深度分析策略设置
    pub order_book_depth: OrderBookDepthStrategySettings,
    
    /// 滑点控制策略设置
    pub slippage_control: SlippageControlStrategySettings,
    
    /// 趋势跟踪策略设置
    pub trend_following: TrendFollowingStrategySettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TwapStrategySettings {
    /// 分割的订单数量
    pub slices: usize,
    /// 每个分割订单之间的间隔（秒）
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderBookDepthStrategySettings {
    /// 要分析的订单簿深度（价格档位数量）
    pub depth_levels: usize,
    /// 最小流动性要求（以基础货币计）
    pub min_liquidity: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlippageControlStrategySettings {
    /// 最大允许的滑点百分比
    pub max_slippage_pct: f64,
    /// 历史价格波动率窗口大小
    pub volatility_window_size: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrendFollowingStrategySettings {
    /// 短期趋势窗口（数据点数量）
    pub short_window: usize,
    /// 长期趋势窗口（数据点数量）
    pub long_window: usize,
    /// 趋势判断阈值（百分比）
    pub trend_threshold: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiskSettings {
    /// 启用的风控组件列表
    pub enabled_controllers: Vec<RiskControllerType>,
    
    /// 每日亏损限制设置
    pub daily_loss_limit: DailyLossLimitSettings,
    
    /// 异常价格保护设置
    pub abnormal_price: AbnormalPriceSettings,
    
    /// 风险敞口控制设置
    pub exposure: ExposureSettings,
    
    /// 交易时间窗口设置
    pub trading_time_window: TradingTimeWindowSettings,
    
    /// 交易频率控制设置
    pub trading_frequency: TradingFrequencySettings,
    
    /// 交易对黑名单设置
    pub pair_blacklist: PairBlacklistSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyLossLimitSettings {
    /// 每日最大亏损金额
    pub max_daily_loss: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbnormalPriceSettings {
    /// 窗口大小（保留的价格记录数量）
    pub window_size: usize,
    /// 异常价格变化阈值（百分比）
    pub abnormal_threshold: f64,
    /// 冷却期（秒），在检测到异常后暂停交易的时间
    pub cooldown_period: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExposureSettings {
    /// 币种最大风险敞口（以USDT计）
    pub max_exposures: Vec<(String, f64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradingTimeWindowSettings {
    /// 允许交易的开始时间 (24小时制，小时)
    pub start_hour: u32,
    /// 允许交易的开始时间 (24小时制，分钟)
    pub start_minute: u32,
    /// 允许交易的结束时间 (24小时制，小时)
    pub end_hour: u32,
    /// 允许交易的结束时间 (24小时制，分钟)
    pub end_minute: u32,
    /// 是否在周末交易
    pub trade_on_weekends: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradingFrequencySettings {
    /// 最小交易间隔（秒）
    pub min_interval_seconds: i64,
    /// 单位时间最大交易次数
    pub max_trades_per_timeframe: usize,
    /// 时间窗口长度（秒）
    pub timeframe_seconds: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairBlacklistSettings {
    /// 黑名单交易对列表
    pub blacklisted_pairs: Vec<String>,
}

impl Default for ArbitrageSettings {
    fn default() -> Self {
        Self {
            min_profit_percentage: 0.1,   // 0.1%最小利润率
            max_trade_amount_usdt: 100.0, // 最大交易金额，USDT
            price_diff_threshold: 0.05,   // 价格差异阈值，百分比
            usdt_symbol: "BTCUSDT".to_string(),
            usdc_symbol: "BTCUSDC".to_string(),
            check_interval_ms: 1000,      // 检查间隔，毫秒
        }
    }
}

impl Default for StrategySettings {
    fn default() -> Self {
        Self {
            enabled_strategies: vec![StrategyType::Simple],
            twap: TwapStrategySettings {
                slices: 5,
                interval_seconds: 60,
            },
            order_book_depth: OrderBookDepthStrategySettings {
                depth_levels: 20,
                min_liquidity: 1.0,
            },
            slippage_control: SlippageControlStrategySettings {
                max_slippage_pct: 0.5,
                volatility_window_size: 20,
            },
            trend_following: TrendFollowingStrategySettings {
                short_window: 10,
                long_window: 30,
                trend_threshold: 1.0,
            },
        }
    }
}

impl Default for RiskSettings {
    fn default() -> Self {
        Self {
            enabled_controllers: vec![
                RiskControllerType::DailyLossLimit,
                RiskControllerType::AbnormalPrice,
            ],
            daily_loss_limit: DailyLossLimitSettings {
                max_daily_loss: 50.0,
            },
            abnormal_price: AbnormalPriceSettings {
                window_size: 30,
                abnormal_threshold: 5.0,
                cooldown_period: 300,
            },
            exposure: ExposureSettings {
                max_exposures: vec![
                    ("BTC".to_string(), 5.0),
                    ("ETH".to_string(), 50.0),
                ],
            },
            trading_time_window: TradingTimeWindowSettings {
                start_hour: 0,
                start_minute: 0,
                end_hour: 23,
                end_minute: 59,
                trade_on_weekends: true,
            },
            trading_frequency: TradingFrequencySettings {
                min_interval_seconds: 30,
                max_trades_per_timeframe: 10,
                timeframe_seconds: 600,
            },
            pair_blacklist: PairBlacklistSettings {
                blacklisted_pairs: vec![],
            },
        }
    }
}

impl Config {
    pub fn new() -> Result<Self> {
        dotenv().ok();
        
        let api_key = env::var("BINANCE_API_KEY")
            .context("BINANCE_API_KEY not set in environment or .env file")?;
        let api_secret = env::var("BINANCE_API_SECRET")
            .context("BINANCE_API_SECRET not set in environment or .env file")?;
        let base_url = env::var("BINANCE_API_URL")
            .unwrap_or_else(|_| "https://api.binance.com".to_string());
            
        Ok(Config {
            api_key,
            api_secret,
            base_url,
            arbitrage_settings: ArbitrageSettings::default(),
            strategy_settings: StrategySettings::default(),
            risk_settings: RiskSettings::default(),
        })
    }
    
    pub fn from_file(path: &str) -> Result<Self> {
        let mut file = File::open(path)
            .context(format!("Failed to open config file: {}", path))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .context(format!("Failed to read config file: {}", path))?;
        
        serde_json::from_str(&contents).context("Failed to parse config JSON")
    }
}
