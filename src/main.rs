mod arbitrage;
mod binance;
mod config;
mod models;
mod strategies;
mod risk;
mod db;
mod analytics;

use arbitrage::ArbitrageEngine;
use binance::{BinanceApi, ExchangeApi, MockBinanceApi};
use clap::{Parser, Subcommand, ArgGroup};
use config::{Config, StrategyType, RiskControllerType};
use dotenv::dotenv;
use db::DatabaseManager;
use analytics::{AnalyticsManager, TimeRange};
use std::path::{PathBuf, Path};
use anyhow::{Context, Result};
use tracing::{info, error, warn, debug, Level};
use tracing_subscriber::FmtSubscriber;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;
use rust_decimal::{Decimal,dec};
use std::str::FromStr;
use std::fs;
use chrono::{DateTime, Utc, Local, NaiveDate, TimeZone};
use rust_decimal::prelude::FromPrimitive;

/// 币安 USDT-USDC 套利程序
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// 配置文件路径
    #[clap(short, long)]
    config_file: Option<PathBuf>,

    /// 日志级别
    #[clap(short, long, default_value = "info")]
    log_level: String,

    /// 基础资产 (例如 BTC, ETH)
    #[clap(short, long, default_value = "BTC")]
    base_asset: String,
    
    /// 数据库连接URL(MySQL)
    #[clap(long)]
    db_url: Option<String>,
    
    /// 启用的交易策略 (多个策略用逗号分隔, 例如 simple,twap)
    #[clap(long)]
    strategies: Option<String>,
    
    /// 启用的风控机制 (多个风控用逗号分隔, 例如 loss-limit,abnormal-price)
    #[clap(long)]
    risk_controllers: Option<String>,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 实时模式，连接实际的币安API
    Live {
        /// 最小利润百分比
        #[clap(long, default_value = "0.1")]
        min_profit: f64,

        /// 最大交易金额 (USDT)
        #[clap(long, default_value = "100")]
        max_amount: f64,

        /// 价格检查间隔 (毫秒)
        #[clap(long, default_value = "1000")]
        interval: u64,
    },
    
    /// 模拟模式，使用模拟数据
    Simulate {
        /// 最小利润百分比
        #[clap(long, default_value = "0.1")]
        min_profit: f64,

        /// 最大交易金额 (USDT)
        #[clap(long, default_value = "100")]
        max_amount: f64,

        /// 价格检查间隔 (毫秒)
        #[clap(long, default_value = "1000")]
        interval: u64,
        
        /// 模拟运行时间 (秒)
        #[clap(long, default_value = "60")]
        runtime: u64,
        
        /// 价格波动率 (百分比)
        #[clap(long, default_value = "1.0")]
        volatility: f64,
        
        /// 创建套利机会的概率 (0-100)
        #[clap(long, default_value = "30")]
        opportunity_probability: u32,
    },
    
    /// 分析历史数据，生成绩效报告
    Analytics {
        /// 分析时间范围: today, yesterday, last7days, last30days, thismonth, lastmonth, alltime
        #[clap(long, default_value = "last7days")]
        time_range: String,
        
        /// 自定义开始日期 (YYYY-MM-DD)
        #[clap(long, requires = "end_date")]
        start_date: Option<String>,
        
        /// 自定义结束日期 (YYYY-MM-DD)
        #[clap(long, requires = "start_date")]
        end_date: Option<String>,
        
        /// 导出报告格式: json, csv
        #[clap(long, default_value = "json")]
        export_format: String,
        
        /// 导出报告路径
        #[clap(long, default_value = "./reports")]
        export_path: PathBuf,
        
        /// 显示币种统计的数量限制
        #[clap(long, default_value = "10")]
        top_assets: i32,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 解析命令行参数
    let args = Args::parse();
    
    // 设置日志
    let log_level = match args.log_level.to_lowercase().as_str() {
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    
    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("无法设置全局日志订阅者");
    
    // 加载环境变量
    dotenv().ok();
    
    // 初始化配置
    let mut config = if let Some(config_path) = &args.config_file {
        Config::from_file(config_path.to_str().unwrap_or(".env"))?
    } else {
        Config::new()?
    };
    
    // 连接数据库（如果提供了连接字符串）
    let db_manager = if let Some(db_url) = &args.db_url {
        match DatabaseManager::new(db_url).await {
            Ok(db) => {
                info!("成功连接到数据库");
                Some(db)
            },
            Err(e) => {
                error!("连接数据库失败: {}", e);
                None
            }
        }
    } else {
        // 尝试从环境变量获取数据库连接字符串
        if let Ok(db_url) = std::env::var("DATABASE_URL") {
            match DatabaseManager::new(&db_url).await {
                Ok(db) => {
                    info!("成功连接到数据库 (使用环境变量DATABASE_URL)");
                    Some(db)
                },
                Err(e) => {
                    error!("连接数据库失败 (使用环境变量DATABASE_URL): {}", e);
                    None
                }
            }
        } else {
            None
        }
    };
    
    match &args.command {
        Command::Analytics { time_range, start_date, end_date, export_format, export_path, top_assets } => {
            // 确保有数据库连接
            let db = match db_manager {
                Some(db) => db,
                None => {
                    return Err(anyhow::anyhow!("分析模式需要数据库连接，请提供 --db-url 参数或设置 DATABASE_URL 环境变量"));
                }
            };
            
            // 创建分析管理器
            let analytics = AnalyticsManager::new(db);
            
            // 解析时间范围
            let range = match time_range.to_lowercase().as_str() {
                "today" => TimeRange::Today,
                "yesterday" => TimeRange::Yesterday,
                "last7days" => TimeRange::Last7Days,
                "last30days" => TimeRange::Last30Days,
                "thismonth" => TimeRange::ThisMonth,
                "lastmonth" => TimeRange::LastMonth,
                "alltime" => TimeRange::AllTime,
                "custom" => {
                    // 解析自定义日期范围
                    if let (Some(start), Some(end)) = (start_date, end_date) {
                        let start_date = NaiveDate::from_str(start)
                            .map_err(|_| anyhow::anyhow!("无效的开始日期格式，应为YYYY-MM-DD"))?;
                        let end_date = NaiveDate::from_str(end)
                            .map_err(|_| anyhow::anyhow!("无效的结束日期格式，应为YYYY-MM-DD"))?;
                            
                        let start_datetime = Local.from_local_date(&start_date).unwrap()
                            .and_hms_opt(0, 0, 0).unwrap().with_timezone(&Utc);
                        let end_datetime = Local.from_local_date(&end_date).unwrap()
                            .and_hms_opt(23, 59, 59).unwrap().with_timezone(&Utc);
                            
                        TimeRange::Custom(start_datetime, end_datetime)
                    } else {
                        return Err(anyhow::anyhow!("自定义时间范围需要同时提供 --start-date 和 --end-date"));
                    }
                },
                _ => {
                    return Err(anyhow::anyhow!("无效的时间范围: {}", time_range));
                }
            };
            
            // 生成报告
            info!("开始生成绩效分析报告 - 时间范围: {}", range.description());
            let report = analytics.generate_report(range).await?;
            
            // 确保导出目录存在
            if !export_path.exists() {
                fs::create_dir_all(&export_path)?;
            }
            
            // 导出报告
            match export_format.to_lowercase().as_str() {
                "json" => {
                    let json_path = export_path.join(format!("report_{}.json", 
                        Local::now().format("%Y%m%d_%H%M%S")));
                    analytics.export_report_to_json(&report, &json_path).await?;
                    info!("报告已导出为JSON格式: {:?}", json_path);
                },
                "csv" => {
                    // CSV格式会导出多个文件
                    let report_dir = export_path.join(format!("report_{}", 
                        Local::now().format("%Y%m%d_%H%M%S")));
                    fs::create_dir_all(&report_dir)?;
                    analytics.export_report_to_csv(&report, &report_dir).await?;
                    info!("报告已导出为CSV格式: {:?}", report_dir);
                },
                _ => {
                    return Err(anyhow::anyhow!("不支持的导出格式: {}", export_format));
                }
            }
            
            // 打印报告摘要
            println!("\n========== 绩效报告摘要 ==========");
            println!("时间范围: {}", report.time_range);
            println!("总交易次数: {}", report.overview.total_trades);
            println!("成功交易次数: {}", report.overview.successful_trades);
            println!("总利润: {:.4} USDT", report.overview.total_profit);
            println!("成功率: {:.2}%", report.success_rate);
            println!("平均每笔利润: {:.4} USDT", report.overview.avg_profit_per_trade);
            println!("=================================\n");
            
            return Ok(());
        },
        _ => {
            // 根据命令行参数更新配置
            match &args.command {
                Command::Live { min_profit, max_amount, interval } | 
                Command::Simulate { min_profit, max_amount, interval, .. } => {
                    config.arbitrage_settings.min_profit_percentage = *min_profit;
                    config.arbitrage_settings.max_trade_amount_usdt = *max_amount;
                    config.arbitrage_settings.check_interval_ms = *interval;
                    
                    // 构造交易对名称
                    config.arbitrage_settings.usdt_symbol = format!("{}{}", args.base_asset, "USDT");
                    config.arbitrage_settings.usdc_symbol = format!("{}{}", args.base_asset, "USDC");
                },
                _ => {}
            }
        }
    }
    
    // 根据命令行参数设置策略
    if let Some(strategies) = &args.strategies {
        let strategy_list: Vec<&str> = strategies.split(',').collect();
        let mut enabled_strategies = Vec::new();
        
        for strategy in strategy_list {
            match strategy.trim().to_lowercase().as_str() {
                "simple" => enabled_strategies.push(StrategyType::Simple),
                "twap" => enabled_strategies.push(StrategyType::TimeWeighted),
                "depth" => enabled_strategies.push(StrategyType::OrderBookDepth),
                "slippage" => enabled_strategies.push(StrategyType::SlippageControl),
                "trend" => enabled_strategies.push(StrategyType::TrendFollowing),
                _ => warn!("未知的策略类型: {}", strategy),
            }
        }
        
        if !enabled_strategies.is_empty() {
            config.strategy_settings.enabled_strategies = enabled_strategies;
        }
    }
    
    // 根据命令行参数设置风控机制
    if let Some(controllers) = &args.risk_controllers {
        let controller_list: Vec<&str> = controllers.split(',').collect();
        let mut enabled_controllers = Vec::new();
        
        for controller in controller_list {
            match controller.trim().to_lowercase().as_str() {
                "loss-limit" => enabled_controllers.push(RiskControllerType::DailyLossLimit),
                "abnormal-price" => enabled_controllers.push(RiskControllerType::AbnormalPrice),
                "exposure" => enabled_controllers.push(RiskControllerType::Exposure),
                "time-window" => enabled_controllers.push(RiskControllerType::TradingTimeWindow),
                "frequency" => enabled_controllers.push(RiskControllerType::TradingFrequency),
                "blacklist" => enabled_controllers.push(RiskControllerType::PairBlacklist),
                _ => warn!("未知的风控类型: {}", controller),
            }
        }
        
        if !enabled_controllers.is_empty() {
            config.risk_settings.enabled_controllers = enabled_controllers;
        }
    }
    
    // 显示程序信息
    info!("币安 USDT-USDC 套利程序启动");
    info!("基础资产: {}", args.base_asset);
    info!("最小利润百分比: {}%", config.arbitrage_settings.min_profit_percentage);
    info!("最大交易金额: {} USDT", config.arbitrage_settings.max_trade_amount_usdt);
    info!("价格检查间隔: {} ms", config.arbitrage_settings.check_interval_ms);
    
    // 显示启用的策略
    info!("启用的交易策略:");
    for strategy in &config.strategy_settings.enabled_strategies {
        info!("  - {:?}", strategy);
    }
    
    // 显示启用的风控机制
    info!("启用的风控机制:");
    for controller in &config.risk_settings.enabled_controllers {
        info!("  - {:?}", controller);
    }
    
    // 显示数据库连接状态
    if db_manager.is_some() {
        info!("数据库连接: 已连接");
    } else {
        info!("数据库连接: 未连接 (套利历史将不会被记录)");
    }
    
    // 根据命令执行相应操作
    match args.command {
        Command::Live { .. } => {
            // 实时模式，使用实际API
            info!("运行模式: 实时");
            let api = BinanceApi::new(config.clone());
            
            let mut engine = ArbitrageEngine::new(api, config, &args.base_asset)?;
            
            // 如果有数据库连接，设置到引擎中
            if let Some(db) = db_manager {
                engine.set_db_manager(db);
            }
            
            // 开始监控套利机会
            info!("开始监控套利机会...");
            engine.monitor_opportunities().await?;
        },
        Command::Simulate { volatility, opportunity_probability, runtime, .. } => {
            // 模拟模式，使用模拟API
            info!("运行模式: 模拟");
            info!("模拟运行时间: {} 秒", runtime);
            info!("价格波动率: {}%", volatility);
            info!("套利机会概率: {}%", opportunity_probability);
            
            let api = MockBinanceApi::new();
            let mut engine = ArbitrageEngine::new(api.clone(), config, &args.base_asset)?;
            
            // 如果有数据库连接，设置到引擎中
            if let Some(db) = db_manager {
                engine.set_db_manager(db);
            }
            
            // 启动价格模拟任务
            let api_clone = api.clone();
            let base_asset = args.base_asset.clone();
            let volatility = volatility;
            let opportunity_prob = opportunity_probability;
            tokio::spawn(async move {
                simulate_price_movements(&api_clone, &base_asset, volatility, opportunity_prob).await;
            });
            
            // 开始监控套利机会，在指定时间后停止
            info!("开始模拟监控套利机会...");
            tokio::select! {
                _ = engine.monitor_opportunities() => {},
                _ = sleep(Duration::from_secs(runtime)) => {
                    info!("模拟时间结束，程序退出");
                }
            }
        },
        Command::Analytics { .. } => {
            // 已在前面处理
        }
    }
    
    Ok(())
}

/// 模拟价格波动
async fn simulate_price_movements(api: &MockBinanceApi, base_asset: &str, volatility: f64, opportunity_probability: u32) {
    // 构造交易对名称
    let usdt_symbol = format!("{}{}", base_asset, "USDT");
    let usdc_symbol = format!("{}{}", base_asset, "USDC");
    
    let mut usdt_price = 50000.0;
    let mut usdc_price = 50025.0;
    let mut rng = rand::thread_rng();
    
    loop {
        // 模拟价格波动，根据设定的波动率
        let volatility_factor = volatility / 100.0;
        let usdt_change = (rng.gen::<f64>() - 0.5) * usdt_price * volatility_factor;
        let usdc_change = (rng.gen::<f64>() - 0.5) * usdc_price * volatility_factor;
        
        usdt_price += usdt_change;
        usdc_price += usdc_change;
        
        // 有指定概率会创造套利机会
        if rng.gen_range(0..100) < opportunity_probability {
            // 随机创造USDT价格低于或高于USDC的情况
            if rng.gen_bool(0.5) {
                usdt_price = usdc_price - rng.gen::<f64>() * 50.0;
            } else {
                usdt_price = usdc_price + rng.gen::<f64>() * 50.0;
            }
        }
        
        // 确保价格不会变为负数
        usdt_price = usdt_price.max(1.0);
        usdc_price = usdc_price.max(1.0);
        
        // 更新API中的价格
        api.update_price(&usdt_symbol, Decimal::from_f64(usdt_price).unwrap_or(dec!(50000)));
        api.update_price(&usdc_symbol, Decimal::from_f64(usdc_price).unwrap_or(dec!(50025)));
        
        debug!("更新模拟价格 - {}: {:.2}, {}: {:.2}", 
            usdt_symbol, usdt_price, 
            usdc_symbol, usdc_price
        );
        
        // 每秒更新一次
        sleep(Duration::from_millis(1000)).await;
    }
}
