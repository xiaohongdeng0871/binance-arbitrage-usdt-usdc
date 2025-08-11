use crate::binance::ExchangeApi;
use crate::config::{Config, StrategyType, RiskControllerType};
use crate::models::{ArbitrageOpportunity, ArbitrageResult, ArbitrageStatus, OrderStatus, Price, QuoteCurrency, Side};
use crate::strategies::{TradingStrategy, SimpleArbitrageStrategy, TimeWeightedAverageStrategy, OrderBookDepthStrategy, SlippageControlStrategy, TrendFollowingStrategy};
use crate::risk::{RiskManager, RiskController, DailyLossLimitController, AbnormalPriceController, ExposureController, TradingTimeWindowController, TradingFrequencyController, PairBlacklistController};
use crate::db::DatabaseManager;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn, error};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use std::collections::HashMap;
use rust_decimal::prelude::FromPrimitive;

/// 套利引擎，使用多种交易策略和风控机制进行USDT和USDC之间的套利
pub struct ArbitrageEngine<T: ExchangeApi + Send + Sync + 'static> {
    api: Arc<T>,
    config: Config,
    base_asset: String,
    strategies: Vec<Box<dyn TradingStrategy>>,
    risk_manager: RiskManager,
    // 添加数据库管理器
    db_manager: Option<Arc<DatabaseManager>>,
}

impl<T: ExchangeApi + Send + Sync + 'static> ArbitrageEngine<T> {
    pub fn new(api: T, config: Config, base_asset: &str) -> Result<Self> {
        // ... existing code ...
        
        // 保留原有的实现代码...
        let api_arc = Arc::new(api);
        
        // 初始化交易策略
        let mut strategies: Vec<Box<dyn TradingStrategy>> = Vec::new();
        
        // 根据配置启用的策略类型初始化相应的策略
        for strategy_type in &config.strategy_settings.enabled_strategies {
            match strategy_type {
                StrategyType::Simple => {
                    info!("启用简单价格差异套利策略");
                    strategies.push(Box::new(SimpleArbitrageStrategy::new(config.clone())));
                },
                StrategyType::TimeWeighted => {
                    info!("启用时间加权平均价格(TWAP)套利策略");
                    let settings = &config.strategy_settings.twap;
                    strategies.push(Box::new(TimeWeightedAverageStrategy::new(
                        config.clone(),
                        settings.slices,
                        settings.interval_seconds,
                    )));
                },
                StrategyType::OrderBookDepth => {
                    info!("启用订单簿深度分析套利策略");
                    let settings = &config.strategy_settings.order_book_depth;
                    strategies.push(Box::new(OrderBookDepthStrategy::new(
                        config.clone(),
                        api_arc.clone(),
                        settings.depth_levels,
                        Decimal::from_f64(settings.min_liquidity).unwrap_or(dec!(1.0)),
                    )));
                },
                StrategyType::SlippageControl => {
                    info!("启用滑点控制套利策略");
                    let settings = &config.strategy_settings.slippage_control;
                    strategies.push(Box::new(SlippageControlStrategy::new(
                        config.clone(),
                        Decimal::from_f64(settings.max_slippage_pct).unwrap_or(dec!(0.5)),
                        settings.volatility_window_size,
                    )));
                },
                StrategyType::TrendFollowing => {
                    info!("启用趋势跟踪套利策略");
                    let settings = &config.strategy_settings.trend_following;
                    strategies.push(Box::new(TrendFollowingStrategy::new(
                        config.clone(),
                        settings.short_window,
                        settings.long_window,
                        Decimal::from_f64(settings.trend_threshold).unwrap_or(dec!(1.0)),
                    )));
                },
            }
        }
        
        // 如果没有启用任何策略，则默认使用简单策略
        if strategies.is_empty() {
            info!("未配置任何策略，使用默认的简单价格差异套利策略");
            strategies.push(Box::new(SimpleArbitrageStrategy::new(config.clone())));
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
                // ... 其他风控初始化代码 ...
                // 保留原有的风控初始化代码...
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
                                timestamp: opportunity.timestamp,
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
    
    // ... existing code ...
    // 保留原有的其他方法实现...

    /// 使用所有启用的策略寻找最佳套利机会
    async fn find_best_arbitrage_opportunity(&self) -> Result<ArbitrageOpportunity> {
        // 构造交易对名称
        let usdt_symbol = format!("{}{}", self.base_asset, "USDT");
        let usdc_symbol = format!("{}{}", self.base_asset, "USDC");
        
        // 获取价格
        let usdt_price = self.api.get_price(&usdt_symbol).await?;
        let usdc_price = self.api.get_price(&usdc_symbol).await?;
        
        debug!("{} 价格: {}", usdt_symbol, usdt_price.price);
        debug!("{} 价格: {}", usdc_symbol, usdc_price.price);
        
        let mut best_opportunity: Option<ArbitrageOpportunity> = None;
        let mut best_profit = Decimal::ZERO;
        
        // 使用每个策略寻找机会
        for strategy in &self.strategies {
            match strategy.find_opportunity(&self.base_asset, &usdt_price, &usdc_price).await {
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
            let max_trade_amount = Decimal::from(self.config.arbitrage_settings.max_trade_amount_usdt);
            
            let opportunity = if usdt_price.price < usdc_price.price {
                // USDT买入，USDC卖出
                ArbitrageOpportunity::new(
                    &self.base_asset,
                    QuoteCurrency::USDT,
                    QuoteCurrency::USDC,
                    usdt_price.price,
                    usdc_price.price,
                    max_trade_amount,
                )
            } else {
                // USDC买入，USDT卖出
                ArbitrageOpportunity::new(
                    &self.base_asset,
                    QuoteCurrency::USDC,
                    QuoteCurrency::USDT,
                    usdc_price.price,
                    usdt_price.price,
                    max_trade_amount,
                )
            };
            
            return Ok(opportunity);
        }
        
        Ok(best_opportunity.unwrap())
    }
    
    /// 执行套利交易
    async fn execute_arbitrage(&self, opportunity: &ArbitrageOpportunity) -> Result<ArbitrageResult> {
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
            status: ArbitrageStatus::Identified,
            timestamp: opportunity.timestamp,
        };
        
        // 构造交易对
        let buy_symbol = format!("{}{}", opportunity.base_asset, opportunity.buy_quote);
        let sell_symbol = format!("{}{}", opportunity.base_asset, opportunity.sell_quote);
        
        info!("执行套利交易 - 买入: {} @ {}, 卖出: {} @ {}, 数量: {}", 
            buy_symbol, opportunity.buy_price,
            sell_symbol, opportunity.sell_price,
            trade_amount_base
        );
        
        // 执行买入订单
        let buy_order = match self.api.place_order(&buy_symbol, Side::Buy, trade_amount_base, None).await {
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
        let mut buy_order_status = buy_order;
        for _ in 0..10 {
            if buy_order_status.status == OrderStatus::Filled {
                break;
            }
            
            sleep(Duration::from_millis(1000)).await;
            buy_order_status = self.api.get_order_status(&buy_symbol, buy_order.order_id).await?;
            info!("买入订单状态: {:?}", buy_order_status.status);
        }
        
        if buy_order_status.status != OrderStatus::Filled {
            info!("取消买入订单...");
            self.api.cancel_order(&buy_symbol, buy_order.order_id).await?;
            result.status = ArbitrageStatus::Failed;
            return Err(anyhow!("买入订单未在预期时间内完成"));
        }
        
        result.status = ArbitrageStatus::BuyOrderFilled;
        
        // 执行卖出订单
        let sell_order = match self.api.place_order(&sell_symbol, Side::Sell, trade_amount_base, None).await {
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
        let mut sell_order_status = sell_order;
        for _ in 0..10 {
            if sell_order_status.status == OrderStatus::Filled {
                break;
            }
            
            sleep(Duration::from_millis(1000)).await;
            sell_order_status = self.api.get_order_status(&sell_symbol, sell_order.order_id).await?;
            info!("卖出订单状态: {:?}", sell_order_status.status);
        }
        
        if sell_order_status.status != OrderStatus::Filled {
            info!("取消卖出订单...");
            self.api.cancel_order(&sell_symbol, sell_order.order_id).await?;
            result.status = ArbitrageStatus::Failed;
            return Err(anyhow!("卖出订单未在预期时间内完成"));
        }
        
        result.status = ArbitrageStatus::Completed;
        
        // 计算实际利润
        let buy_total = trade_amount_base * buy_order_status.price;
        let sell_total = trade_amount_base * sell_order_status.price;
        let profit = sell_total - buy_total;
        
        result.profit = profit;
        
        info!("套利交易完成! 利润: {}", profit);
        Ok(result)
    }
}
