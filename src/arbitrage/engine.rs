use crate::exchanges::ExchangeApi;
use crate::config::{Config, StrategyType, RiskControllerType};
use crate::models::{ArbitrageOpportunity, ArbitrageResult, ArbitrageStatus, OrderStatus, QuoteCurrency, Side, FundingRate};
use crate::strategies::{TradingStrategy, SimpleArbitrageStrategy, TimeWeightedAverageStrategy, OrderBookDepthStrategy, SlippageControlStrategy, TrendFollowingStrategy, FundingRateArbitrageStrategy};
use crate::risk::{RiskManager, DailyLossLimitController, AbnormalPriceController, ExposureController, TradingTimeWindowController, TradingFrequencyController, PairBlacklistController};
use crate::db::DatabaseManager;
use anyhow::{anyhow, Result};
use log::{debug, info, warn, error};
use rust_decimal::{dec, Decimal};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use chrono::Utc;
use rust_decimal::prelude::FromPrimitive;

/// 套利策略枚举，包含所有支持的套利策略类型
#[derive(Debug, Clone)]
pub enum ArbitrageStrategy {
    /// 简单价格差异套利策略
    Simple(Box<SimpleArbitrageStrategy>),
    /// 时间加权平均价格(TWAP)套利策略
    TimeWeighted(Box<TimeWeightedAverageStrategy>),
    /// 订单簿深度分析套利策略
    OrderBookDepth(Box<OrderBookDepthStrategy>),
    /// 滑点控制套利策略
    SlippageControl(Box<SlippageControlStrategy>),
    /// 趋势跟踪套利策略
    TrendFollowing(Box<TrendFollowingStrategy>),
    /// 资金费率套利策略
    FundingRate(Box<FundingRateArbitrageStrategy>),
}

impl ArbitrageStrategy {
    /// 获取策略名称
    pub fn name(&self) -> &str {
        match self {
            ArbitrageStrategy::Simple(_) => "Simple",
            ArbitrageStrategy::TimeWeighted(_) => "TimeWeighted",
            ArbitrageStrategy::OrderBookDepth(_) => "OrderBookDepth",
            ArbitrageStrategy::SlippageControl(_) => "SlippageControl",
            ArbitrageStrategy::TrendFollowing(_) => "TrendFollowing",
            ArbitrageStrategy::FundingRate(_) => "FundingRate",
        }
    }

    /// 寻找套利机会
    pub async fn find_opportunity(
        &self,
        base_asset: &str,
        spot_price: &crate::models::Price,
        futures_price: &crate::models::Price,
    ) -> Result<Option<ArbitrageOpportunity>> {
        match self {
            ArbitrageStrategy::Simple(strategy) => strategy.find_opportunity(base_asset, spot_price, futures_price).await,
            ArbitrageStrategy::TimeWeighted(strategy) => strategy.find_opportunity(base_asset, spot_price, futures_price).await,
            ArbitrageStrategy::OrderBookDepth(strategy) => strategy.find_opportunity(base_asset, spot_price, futures_price).await,
            ArbitrageStrategy::SlippageControl(strategy) => strategy.find_opportunity(base_asset, spot_price, futures_price).await,
            ArbitrageStrategy::TrendFollowing(strategy) => strategy.find_opportunity(base_asset, spot_price, futures_price).await,
            ArbitrageStrategy::FundingRate(strategy) => strategy.find_opportunity(base_asset, spot_price, futures_price).await,
        }
    }

    /// 验证套利机会是否满足策略要求
    pub async fn validate_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<bool> {
        match self {
            ArbitrageStrategy::Simple(strategy) => strategy.validate_opportunity(opportunity).await,
            ArbitrageStrategy::TimeWeighted(strategy) => strategy.validate_opportunity(opportunity).await,
            ArbitrageStrategy::OrderBookDepth(strategy) => strategy.validate_opportunity(opportunity).await,
            ArbitrageStrategy::SlippageControl(strategy) => strategy.validate_opportunity(opportunity).await,
            ArbitrageStrategy::TrendFollowing(strategy) => strategy.validate_opportunity(opportunity).await,
            ArbitrageStrategy::FundingRate(strategy) => strategy.validate_opportunity(opportunity).await,
        }
    }
}

pub struct ArbitrageEngine {
    api: Arc<Box<dyn ExchangeApi>>,
    config: Config,
    base_asset: String,
    strategies: Vec<ArbitrageStrategy>,
    risk_manager: RiskManager,
    // 添加数据库管理器
    db_manager: Option<Arc<DatabaseManager>>,
}

impl ArbitrageEngine {
    pub fn new(api: Box<dyn ExchangeApi>, config: Config, base_asset: &str) -> Result<Self> {
        let api_arc = Arc::new(api);
        
        let mut strategies: Vec<ArbitrageStrategy> = Vec::new();
        
        // 根据配置启用的策略类型初始化相应的策略
        for strategy_type in &config.strategy_settings.enabled_strategies {
            match strategy_type {
                StrategyType::Simple => {
                    info!("启用简单价格差异套利策略");
                    strategies.push(ArbitrageStrategy::Simple(Box::new(SimpleArbitrageStrategy::new(config.clone()))));
                },
                StrategyType::TimeWeighted => {
                    info!("启用时间加权平均价格(TWAP)套利策略");
                    let settings = &config.strategy_settings.twap;
                    strategies.push(ArbitrageStrategy::TimeWeighted(Box::new(TimeWeightedAverageStrategy::new(
                        config.clone(),
                        settings.slices,
                        settings.interval_seconds,
                    ))));
                },
                StrategyType::OrderBookDepth => {
                    info!("启用订单簿深度分析套利策略");
                    let settings = &config.strategy_settings.order_book_depth;
                    strategies.push(ArbitrageStrategy::OrderBookDepth(Box::new(OrderBookDepthStrategy::new(
                        config.clone(),
                        settings.depth_levels,
                        Decimal::from_f64(settings.min_liquidity).unwrap_or(dec!(1.0)),
                    ))));
                },
                StrategyType::SlippageControl => {
                    info!("启用滑点控制套利策略");
                    let settings = &config.strategy_settings.slippage_control;
                    strategies.push(ArbitrageStrategy::SlippageControl(Box::new(SlippageControlStrategy::new(
                        config.clone(),
                        Decimal::from_f64(settings.max_slippage_pct).unwrap_or(dec!(0.5)),
                        settings.volatility_window_size,
                    ))));
                },
                StrategyType::TrendFollowing => {
                    info!("启用趋势跟踪套利策略");
                    let settings = &config.strategy_settings.trend_following;
                    strategies.push(ArbitrageStrategy::TrendFollowing(Box::new(TrendFollowingStrategy::new(
                        config.clone(),
                        settings.short_window,
                        settings.long_window,
                        Decimal::from_f64(settings.trend_threshold).unwrap_or(dec!(1.0)),
                    ))));
                },
                StrategyType::FundingRateArbitrage => {
                    info!("启用资金费率套利策略");
                    let settings = &config.strategy_settings.funding_rate;
                    strategies.push(ArbitrageStrategy::FundingRate(Box::new(FundingRateArbitrageStrategy::new(
                        config.clone(),
                        Decimal::from_f64(settings.min_funding_rate_diff).unwrap_or(dec!(0.01)),
                    ))));
                },
            }
        }
        
        // 如果没有启用任何策略，则返回错误
        if strategies.is_empty() {
            return Err(anyhow!("未配置任何交易策略，请在配置文件或命令行参数中至少指定一种策略"));
        }
        
        // 初始化风控管理器
        let mut risk_manager = RiskManager::new(config.clone());
        
        // 根据配置启用的风控类型初始化相应的控制器
        for controller_type in &config.risk_settings.enabled_controllers {
            match controller_type {
                RiskControllerType::DailyLossLimit => {
                    info!("启用每日亏损限制风控");
                    risk_manager.add_controller(DailyLossLimitController::new(
                        Decimal::from_f64(config.risk_settings.daily_loss_limit.max_daily_loss).unwrap_or(dec!(50.0))
                    ));
                },
                RiskControllerType::AbnormalPrice => {
                    info!("启用异常价格保护风控");
                    let settings = &config.risk_settings.abnormal_price;
                    risk_manager.add_controller(AbnormalPriceController::new(
                        settings.window_size,
                        Decimal::from_f64(settings.abnormal_threshold).unwrap_or(dec!(5.0)),
                        settings.cooldown_period,
                    ));
                },
                RiskControllerType::Exposure => {
                    info!("启用风险敞口控制风控");
                    let mut exposure_controller = ExposureController::new(api_arc.clone());
                    
                    // 设置每种币的最大风险敞口
                    for (asset, max_exposure) in &config.risk_settings.exposure.max_exposures {
                        exposure_controller.set_max_exposure(
                            asset, 
                            Decimal::from_f64(*max_exposure).unwrap_or(Decimal::MAX)
                        );
                    }
                    
                    risk_manager.add_controller(exposure_controller);
                },
                RiskControllerType::TradingTimeWindow => {
                    info!("启用交易时间窗口风控");
                    let settings = &config.risk_settings.trading_time_window;
                    
                    if let Ok(controller) = TradingTimeWindowController::new(
                        settings.start_hour,
                        settings.start_minute,
                        settings.end_hour,
                        settings.end_minute,
                        settings.trade_on_weekends,
                    ) {
                        risk_manager.add_controller(controller);
                    } else {
                        warn!("无法创建交易时间窗口控制器，时间设置无效");
                    }
                },
                RiskControllerType::TradingFrequency => {
                    info!("启用交易频率控制风控");
                    let settings = &config.risk_settings.trading_frequency;
                    risk_manager.add_controller(TradingFrequencyController::new(
                        settings.min_interval_seconds,
                        settings.max_trades_per_timeframe,
                        settings.timeframe_seconds,
                    ));
                },
                RiskControllerType::PairBlacklist => {
                    info!("启用交易对黑名单风控");
                    let controller = PairBlacklistController::new();
                    
                    // 添加黑名单交易对
                    for pair in &config.risk_settings.pair_blacklist.blacklisted_pairs {
                        let pair_str = pair.as_str();
                        
                        if pair_str.ends_with("USDT") {
                            let base = &pair_str[0..pair_str.len() - 4];
                            controller.add_to_blacklist(base, "USDT");
                        } else if pair_str.ends_with("USDC") {
                            let base = &pair_str[0..pair_str.len() - 4];
                            controller.add_to_blacklist(base, "USDC");
                        } else {
                            warn!("无效的交易对格式: {}, 应该以USDT或USDC结尾", pair);
                        }
                    }
                    
                    risk_manager.add_controller(controller);
                },
            }
        }
        
        Ok(Self {
            api: api_arc,
            config,
            base_asset: base_asset.to_string(),
            strategies,
            risk_manager,
            db_manager: None,
        })
    }

    /// 设置数据库管理器
    pub fn set_db_manager(&mut self, db_manager: DatabaseManager) {
        self.db_manager = Some(Arc::new(db_manager));
        info!("已设置数据库管理器，套利结果将被记录");
    }

    /// 持续监控币对价格，寻找套利机会
    pub async fn monitor_opportunities(&self) -> Result<()> {
        info!("开始监控 {}-USDT/USDC 套利机会", self.base_asset);
        
        loop {
            if let Ok(opportunity) = self.find_best_arbitrage_opportunity().await {
                // 验证风控规则
                let (is_valid, rejection_reasons) = self.risk_manager.validate_opportunity(&opportunity).await?;
                
                if !is_valid {
                    for reason in rejection_reasons {
                        warn!("风控拒绝: {}", reason);
                    }
                    debug!("套利机会被风控拒绝，跳过");
                } else {
                    // 如果通过风控，执行套利
                    info!(
                        "发现套利机会: {} 买入: {} {}, 卖出: {} {}, 价差: {}, 利润率: {}%",
                        opportunity.base_asset,
                        opportunity.buy_quote,
                        opportunity.buy_price,
                        opportunity.sell_quote,
                        opportunity.sell_price,
                        opportunity.price_diff,
                        opportunity.profit_percentage
                    );
                    
                    match self.execute_arbitrage(&opportunity).await {
                        Ok(result) => {
                            info!(
                                "套利完成: {} 利润: {} ({}%)",
                                result.base_asset, result.profit, result.profit_percentage
                            );
                            
                            // 记录交易结果
                            self.risk_manager.record_result(&result).await?;
                            
                            // 如果设置了数据库，保存套利结果
                            if let Some(db) = &self.db_manager {
                                match db.record_arbitrage_result(&result).await {
                                    Ok(id) => {
                                        info!("已记录套利结果到数据库: ID={}", id);
                                    },
                                    Err(e) => {
                                        error!("记录套利结果到数据库失败: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("套利执行失败: {}", e);
                            
                            // 创建失败结果并记录
                            let failed_result = ArbitrageResult {
                                base_asset: opportunity.base_asset.clone(),
                                buy_quote: opportunity.buy_quote.to_string(),
                                sell_quote: opportunity.sell_quote.to_string(),
                                buy_price: opportunity.buy_price,
                                sell_price: opportunity.sell_price,
                                trade_amount: Decimal::ZERO,
                                profit: Decimal::ZERO,
                                profit_percentage: Decimal::ZERO,
                                buy_order_id: None,
                                sell_order_id: None,
                                status: ArbitrageStatus::Failed,
                                start_time: opportunity.timestamp,
                                end_time: Some(Utc::now()),
                                buy_funding_rate: opportunity.buy_funding_rate,
                                sell_funding_rate: opportunity.sell_funding_rate,
                            };
                            
                            self.risk_manager.record_result(&failed_result).await?;
                            
                            // 如果设置了数据库，保存失败记录
                            if let Some(db) = &self.db_manager {
                                if let Err(e) = db.record_arbitrage_result(&failed_result).await {
                                    error!("记录失败的套利结果到数据库失败: {}", e);
                                }
                            }
                        }
                    }
                }
            }
            
            // 等待指定的时间间隔
            sleep(Duration::from_millis(self.config.arbitrage_settings.check_interval_ms)).await;
        }
    }
    
    /// 使用所有启用的策略寻找最佳套利机会
    async fn find_best_arbitrage_opportunity(&self) -> Result<ArbitrageOpportunity> {
        // 构造交易对名称
        let spot_symbol = format!("{}{}", self.base_asset, "USDT");
        let futures_symbol = format!("{}{}", self.base_asset, "USDT"); // 合约交易对
        
        // 获取现货价格
        let spot_price = self.api.get_spot_price(&spot_symbol).await?;
        
        // 获取合约价格
        let futures_price = self.api.get_futures_price(&futures_symbol).await?;
        
        debug!("{} 现货价格: {}", spot_symbol, spot_price.price);
        debug!("{} 合约价格: {}", futures_symbol, futures_price.price);
        
        // 获取资金费率
        let spot_funding_rate = self.api.get_funding_rate(&spot_symbol).await.unwrap_or_else(|_| {
            warn!("无法获取 {} 资金费率", spot_symbol);
            FundingRate {
                symbol: spot_symbol.clone(),
                funding_rate: Decimal::ZERO,
                timestamp: Utc::now(),
            }
        });
        
        let futures_funding_rate = self.api.get_funding_rate(&futures_symbol).await.unwrap_or_else(|_| {
            warn!("无法获取 {} 资金费率", futures_symbol);
            FundingRate {
                symbol: futures_symbol.clone(),
                funding_rate: Decimal::ZERO,
                timestamp: Utc::now(),
            }
        });
        
        debug!("{} 现货资金费率: {}", spot_symbol, spot_funding_rate.funding_rate);
        debug!("{} 合约资金费率: {}", futures_symbol, futures_funding_rate.funding_rate);
        
        let mut best_opportunity: Option<ArbitrageOpportunity> = None;
        let mut best_profit = Decimal::ZERO;
        
        // 使用每个策略寻找机会
        for strategy in &self.strategies {
            match strategy.find_opportunity(&self.base_asset, &spot_price, &futures_price).await {
                Ok(Some(opportunity)) => {
                    // 验证是否符合策略要求
                    match strategy.validate_opportunity(&opportunity).await {
                        Ok(true) => {
                            if opportunity.profit_percentage > best_profit {
                                best_profit = opportunity.profit_percentage;
                                debug!(
                                    "发现更优套利机会 (策略: {}): 利润率 {}%, 价差: {}",
                                    strategy.name(), opportunity.profit_percentage, opportunity.price_diff
                                );
                                best_opportunity = Some(opportunity);
                            }
                        },
                        Ok(false) => {
                            debug!(
                                "策略 {} 发现机会但验证失败: 利润率 {}% 不足",
                                strategy.name(), opportunity.profit_percentage
                            );
                        },
                        Err(e) => {
                            warn!("策略 {} 验证出错: {}", strategy.name(), e);
                        }
                    }
                },
                Ok(None) => {
                    debug!("策略 {} 未发现有效套利机会", strategy.name());
                },
                Err(e) => {
                    warn!("策略 {} 寻找机会出错: {}", strategy.name(), e);
                }
            }
        }
        
        // 如果没有找到任何机会，创建一个基本的机会（默认使用简单策略的逻辑）
        if best_opportunity.is_none() {
            let max_trade_amount = Decimal::from_f64(self.config.arbitrage_settings.max_trade_amount_usdt).unwrap();
            
            let opportunity = ArbitrageOpportunity::new_with_funding_rates(
                &self.base_asset,
                QuoteCurrency::USDT,
                QuoteCurrency::USDT,
                spot_price.price,
                futures_price.price,
                max_trade_amount,
                spot_funding_rate.funding_rate,
                futures_funding_rate.funding_rate,
            );
            
            return Ok(opportunity);
        }
        
        // 为找到的最佳机会添加资金费率信息
        let mut opportunity = best_opportunity.unwrap();
        opportunity.buy_funding_rate = Some(spot_funding_rate.funding_rate);
        opportunity.sell_funding_rate = Some(futures_funding_rate.funding_rate);
        
        Ok(opportunity)
    }
    
    /// 执行套利交易
    async fn execute_arbitrage(&self, opportunity: &ArbitrageOpportunity) -> Result<ArbitrageResult> {
        // 根据策略类型执行不同的套利逻辑
        if self.strategies.iter().any(|s| s.name() == "FundingRate") {
            return self.execute_funding_rate_arbitrage(opportunity).await;
        }
        
        // 默认执行标准套利逻辑
        self.execute_standard_arbitrage(opportunity).await
    }
    
    /// 执行标准套利交易
    async fn execute_standard_arbitrage(&self, opportunity: &ArbitrageOpportunity) -> Result<ArbitrageResult> {
        // 计算交易量
        let trade_amount_quote = opportunity.max_trade_amount;
        let trade_amount_base = trade_amount_quote / opportunity.buy_price;
        
        let mut result = ArbitrageResult {
            base_asset: opportunity.base_asset.clone(),
            buy_quote: opportunity.buy_quote.to_string(),
            sell_quote: opportunity.sell_quote.to_string(),
            buy_price: opportunity.buy_price,
            sell_price: opportunity.sell_price,
            trade_amount: trade_amount_base,
            profit: Decimal::ZERO,
            profit_percentage: opportunity.profit_percentage,
            buy_order_id: None,
            sell_order_id: None,
            status: ArbitrageStatus::Executing,
            start_time: Utc::now(),
            end_time: None,
            buy_funding_rate: opportunity.buy_funding_rate,
            sell_funding_rate: opportunity.sell_funding_rate,
        };
        
        // 构造交易对
        let spot_symbol = format!("{}{}", opportunity.base_asset, "USDT");
        let futures_symbol = format!("{}{}", opportunity.base_asset, "USDT");
        
        info!("执行套利交易 - 现货: {} @ {}, 合约: {} @ {}, 数量: {}", 
            spot_symbol, opportunity.buy_price,
            futures_symbol, opportunity.sell_price,
            trade_amount_base
        );
        
        if let (Some(spot_funding_rate), Some(futures_funding_rate)) = (opportunity.buy_funding_rate, opportunity.sell_funding_rate) {
            info!("资金费率 - 现货: {} ({}), 合约: {} ({})", 
                spot_symbol, spot_funding_rate,
                futures_symbol, futures_funding_rate
            );
        }
        
        // 执行买入订单
        let buy_order = match self.api.place_order(&spot_symbol, Side::Buy, trade_amount_base, None).await {
            Ok(order) => {
                info!("买入订单已提交: ID={}, 状态={:?}", order.order_id, order.status);
                result.buy_order_id = Some(order.order_id);
                result.status = ArbitrageStatus::BuyOrderPlaced;
                order
            },
            Err(e) => {
                result.status = ArbitrageStatus::Failed;
                return Err(anyhow!("买入订单失败: {}", e));
            }
        };
        
        // 等待买入订单完成
        let mut buy_order_status = buy_order.clone();
        for _ in 0..10 {
            if buy_order_status.status == OrderStatus::Filled {
                break;
            }
            
            sleep(Duration::from_millis(1000)).await;
            buy_order_status = self.api.get_order_status(&spot_symbol, buy_order.order_id).await?;
            info!("买入订单状态: {:?}", buy_order_status.status);
        }
        
        if buy_order_status.status != OrderStatus::Filled {
            info!("取消买入订单...");
            self.api.cancel_order(&spot_symbol, buy_order.order_id).await?;
            result.status = ArbitrageStatus::Failed;
            return Err(anyhow!("买入订单未在预期时间内完成"));
        }
        
        result.status = ArbitrageStatus::BuyOrderFilled;
        
        // 执行卖出订单
        let sell_order = match self.api.place_order(&futures_symbol, Side::Sell, trade_amount_base, None).await {
            Ok(order) => {
                info!("卖出订单已提交: ID={}, 状态={:?}", order.order_id, order.status);
                result.sell_order_id = Some(order.order_id);
                result.status = ArbitrageStatus::SellOrderPlaced;
                order
            },
            Err(e) => {
                result.status = ArbitrageStatus::Failed;
                return Err(anyhow!("卖出订单失败: {}", e));
            }
        };
        
        // 等待卖出订单完成
        let mut sell_order_status = sell_order.clone();
        for _ in 0..10 {
            if sell_order_status.status == OrderStatus::Filled {
                break;
            }
            
            sleep(Duration::from_millis(1000)).await;
            sell_order_status = self.api.get_order_status(&futures_symbol, sell_order.order_id).await?;
            info!("卖出订单状态: {:?}", sell_order_status.status);
        }
        
        if sell_order_status.status != OrderStatus::Filled {
            info!("取消卖出订单...");
            self.api.cancel_order(&futures_symbol, sell_order.order_id).await?;
            result.status = ArbitrageStatus::Failed;
            return Err(anyhow!("卖出订单未在预期时间内完成"));
        }
        
        result.status = ArbitrageStatus::Completed;
        result.end_time = Some(Utc::now());
        
        // 计算实际利润
        let buy_total = trade_amount_base * buy_order_status.price;
        let sell_total = trade_amount_base * sell_order_status.price;
        let profit = sell_total - buy_total;
        
        result.profit = profit;
        
        info!("套利交易完成! 利润: {}", profit);
        Ok(result)
    }
    
    /// 执行资金费率套利交易（现货和合约对冲）
    async fn execute_funding_rate_arbitrage(&self, opportunity: &ArbitrageOpportunity) -> Result<ArbitrageResult> {
        info!("执行资金费率套利交易，现货和合约对冲");
        
        // 计算交易量
        let trade_amount_quote = opportunity.max_trade_amount;
        let trade_amount_base = trade_amount_quote / opportunity.buy_price;
        
        let mut result = ArbitrageResult {
            base_asset: opportunity.base_asset.clone(),
            buy_quote: opportunity.buy_quote.to_string(),
            sell_quote: opportunity.sell_quote.to_string(),
            buy_price: opportunity.buy_price,
            sell_price: opportunity.sell_price,
            trade_amount: trade_amount_base,
            profit: Decimal::ZERO,
            profit_percentage: opportunity.profit_percentage,
            buy_order_id: None,
            sell_order_id: None,
            status: ArbitrageStatus::Executing,
            start_time: Utc::now(),
            end_time: None,
            buy_funding_rate: opportunity.buy_funding_rate,
            sell_funding_rate: opportunity.sell_funding_rate,
        };
        
        // 构造交易对
        let spot_symbol = format!("{}{}", opportunity.base_asset, "USDT");
        let futures_symbol = format!("{}{}", opportunity.base_asset, "USDT");
        
        // 获取现货和合约价格用于价差分析
        let spot_price = self.api.get_spot_price(&spot_symbol).await.unwrap_or_else(|_| {
            warn!("无法获取 {} 现货价格", spot_symbol);
            crate::models::Price {
                symbol: spot_symbol.clone(),
                price: opportunity.buy_price,
                timestamp: Utc::now(),
            }
        });
        
        let futures_price = self.api.get_futures_price(&futures_symbol).await.unwrap_or_else(|_| {
            warn!("无法获取 {} 合约价格", futures_symbol);
            crate::models::Price {
                symbol: futures_symbol.clone(),
                price: opportunity.sell_price,
                timestamp: Utc::now(),
            }
        });
        
        // 计算现货和合约价格差
        let spot_futures_diff = (spot_price.price - futures_price.price).abs();
        let spot_futures_diff_pct = spot_futures_diff / spot_price.price * Decimal::from(100);
        
        info!("{} 现货价格: {}, 合约价格: {}, 价差: {} ({}%)", 
            spot_symbol, spot_price.price, futures_price.price, spot_futures_diff, spot_futures_diff_pct);
        
        // 检查价差是否在可接受范围内
        if spot_futures_diff_pct > Decimal::from(1) { // 1%作为最大可接受价差
            result.status = ArbitrageStatus::Failed;
            return Err(anyhow!("现货和合约价格差过大({}%)，放弃套利", spot_futures_diff_pct));
        }
        
        // 确定资金费率方向
        let (spot_funding_rate, futures_funding_rate) = match (opportunity.buy_funding_rate, opportunity.sell_funding_rate) {
            (Some(spot_rate), Some(futures_rate)) => (spot_rate, futures_rate),
            _ => {
                result.status = ArbitrageStatus::Failed;
                return Err(anyhow!("缺少资金费率信息"));
            }
        };
        
        // 根据资金费率方向决定交易方向
        // 如果合约资金费率 > 现货资金费率，应该在现货买入，在合约卖出
        // 如果合约资金费率 < 现货资金费率，应该在现货卖出，在合约买入
        let funding_rate_diff = futures_funding_rate - spot_funding_rate;
        
        info!("资金费率差异: {} - {} = {}", futures_funding_rate, spot_funding_rate, funding_rate_diff);
        
        if funding_rate_diff > Decimal::ZERO {
            // 合约资金费率更高，应该在现货买入，在合约卖出（正资金费率套利）
            info!("资金费率方向: 在现货买入，在合约卖出");
            
            // 执行现货买入订单
            let spot_buy_order = match self.api.place_order(&spot_symbol, Side::Buy, trade_amount_base, None).await {
                Ok(order) => {
                    info!("现货买入订单已提交: ID={}, 状态={:?}", order.order_id, order.status);
                    result.buy_order_id = Some(order.order_id);
                    result.status = ArbitrageStatus::BuyOrderPlaced;
                    order
                },
                Err(e) => {
                    result.status = ArbitrageStatus::Failed;
                    return Err(anyhow!("现货买入订单失败: {}", e));
                }
            };
            
            // 等待现货买入订单完成
            let mut spot_buy_order_status = spot_buy_order.clone();
            for _ in 0..10 {
                if spot_buy_order_status.status == OrderStatus::Filled {
                    break;
                }
                
                sleep(Duration::from_millis(1000)).await;
                spot_buy_order_status = self.api.get_order_status(&spot_symbol, spot_buy_order.order_id).await?;
                info!("现货买入订单状态: {:?}", spot_buy_order_status.status);
            }
            
            if spot_buy_order_status.status != OrderStatus::Filled {
                info!("取消现货买入订单...");
                self.api.cancel_order(&spot_symbol, spot_buy_order.order_id).await?;
                result.status = ArbitrageStatus::Failed;
                return Err(anyhow!("现货买入订单未在预期时间内完成"));
            }
            
            result.status = ArbitrageStatus::BuyOrderFilled;
            
            // 执行合约卖出订单
            let futures_sell_order = match self.api.place_futures_order(&futures_symbol, Side::Sell, trade_amount_base, None).await {
                Ok(order) => {
                    info!("合约卖出订单已提交: ID={}, 状态={:?}", order.order_id, order.status);
                    result.sell_order_id = Some(order.order_id);
                    result.status = ArbitrageStatus::SellOrderPlaced;
                    order
                },
                Err(e) => {
                    result.status = ArbitrageStatus::Failed;
                    return Err(anyhow!("合约卖出订单失败: {}", e));
                }
            };
            
            // 等待合约卖出订单完成
            let mut futures_sell_order_status = futures_sell_order.clone();
            for _ in 0..10 {
                if futures_sell_order_status.status == OrderStatus::Filled {
                    break;
                }
                
                sleep(Duration::from_millis(1000)).await;
                futures_sell_order_status = self.api.get_futures_order_status(&futures_symbol, futures_sell_order.order_id).await?;
                info!("合约卖出订单状态: {:?}", futures_sell_order_status.status);
            }
            
            if futures_sell_order_status.status != OrderStatus::Filled {
                info!("取消合约卖出订单...");
                self.api.cancel_futures_order(&futures_symbol, futures_sell_order.order_id).await?;
                result.status = ArbitrageStatus::Failed;
                return Err(anyhow!("合约卖出订单未在预期时间内完成"));
            }
            
            result.status = ArbitrageStatus::Completed;
            result.end_time = Some(Utc::now());
            
            // 计算实际利润（现货买入价格和合约卖出价格）
            let spot_buy_total = trade_amount_base * spot_buy_order_status.price;
            let futures_sell_total = trade_amount_base * futures_sell_order_status.price;
            let profit = futures_sell_total - spot_buy_total;
            
            result.profit = profit;
            
            info!("资金费率套利交易完成! 利润: {}", profit);
            Ok(result)
        } else {
            // 合约资金费率更低，应该在现货卖出，在合约买入（负资金费率套利）
            info!("资金费率方向: 在现货卖出，在合约买入");
            
            // 执行现货卖出订单
            let spot_sell_order = match self.api.place_order(&spot_symbol, Side::Sell, trade_amount_base, None).await {
                Ok(order) => {
                    info!("现货卖出订单已提交: ID={}, 状态={:?}", order.order_id, order.status);
                    result.sell_order_id = Some(order.order_id);
                    result.status = ArbitrageStatus::SellOrderPlaced;
                    order
                },
                Err(e) => {
                    result.status = ArbitrageStatus::Failed;
                    return Err(anyhow!("现货卖出订单失败: {}", e));
                }
            };
            
            // 等待现货卖出订单完成
            let mut spot_sell_order_status = spot_sell_order.clone();
            for _ in 0..10 {
                if spot_sell_order_status.status == OrderStatus::Filled {
                    break;
                }
                
                sleep(Duration::from_millis(1000)).await;
                spot_sell_order_status = self.api.get_order_status(&spot_symbol, spot_sell_order.order_id).await?;
                info!("现货卖出订单状态: {:?}", spot_sell_order_status.status);
            }
            
            if spot_sell_order_status.status != OrderStatus::Filled {
                info!("取消现货卖出订单...");
                self.api.cancel_order(&spot_symbol, spot_sell_order.order_id).await?;
                result.status = ArbitrageStatus::Failed;
                return Err(anyhow!("现货卖出订单未在预期时间内完成"));
            }
            
            result.status = ArbitrageStatus::SellOrderFilled;
            
            // 执行合约买入订单
            let futures_buy_order = match self.api.place_futures_order(&futures_symbol, Side::Buy, trade_amount_base, None).await {
                Ok(order) => {
                    info!("合约买入订单已提交: ID={}, 状态={:?}", order.order_id, order.status);
                    result.buy_order_id = Some(order.order_id);
                    result.status = ArbitrageStatus::BuyOrderPlaced;
                    order
                },
                Err(e) => {
                    result.status = ArbitrageStatus::Failed;
                    return Err(anyhow!("合约买入订单失败: {}", e));
                }
            };
            
            // 等待合约买入订单完成
            let mut futures_buy_order_status = futures_buy_order.clone();
            for _ in 0..10 {
                if futures_buy_order_status.status == OrderStatus::Filled {
                    break;
                }
                
                sleep(Duration::from_millis(1000)).await;
                futures_buy_order_status = self.api.get_futures_order_status(&futures_symbol, futures_buy_order.order_id).await?;
                info!("合约买入订单状态: {:?}", futures_buy_order_status.status);
            }
            
            if futures_buy_order_status.status != OrderStatus::Filled {
                info!("取消合约买入订单...");
                self.api.cancel_futures_order(&futures_symbol, futures_buy_order.order_id).await?;
                result.status = ArbitrageStatus::Failed;
                return Err(anyhow!("合约买入订单未在预期时间内完成"));
            }
            
            result.status = ArbitrageStatus::Completed;
            result.end_time = Some(Utc::now());
            
            // 计算实际利润（现货卖出价格和合约买入价格）
            let spot_sell_total = trade_amount_base * spot_sell_order_status.price;
            let futures_buy_total = trade_amount_base * futures_buy_order_status.price;
            let profit = spot_sell_total - futures_buy_total;
            
            result.profit = profit;
            
            info!("资金费率套利交易完成! 利润: {}", profit);
            Ok(result)
        }
    }
}