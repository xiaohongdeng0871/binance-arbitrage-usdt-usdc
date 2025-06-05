//! 数据库模块，负责与MySQL交互并提供套利历史和绩效数据的存储与检索

use anyhow::{Context, Result, anyhow};
use sqlx::{MySql, MySqlPool, Pool};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::models::{ArbitrageResult, ArbitrageStatus};
use chrono::{DateTime, Utc, NaiveDateTime, Duration, TimeZone};
use log::{info, warn, error, debug};
use rust_decimal::Decimal;
use serde::{Serialize, Deserialize};

/// 数据库连接管理器
pub struct DatabaseManager {
    pool: Arc<MySqlPool>,
    last_flush: Arc<Mutex<Instant>>,
}

/// 交易统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeStats {
    pub total_trades: i64,
    pub successful_trades: i64,
    pub failed_trades: i64,
    pub total_profit: Decimal,
    pub total_volume: Decimal,
    pub avg_profit_per_trade: Decimal,
    pub max_profit: Decimal,
    pub max_loss: Decimal,
    pub avg_trade_duration_ms: i64,
}

/// 每日交易统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub date: String,
    pub trades: i64,
    pub profit: Decimal,
    pub volume: Decimal,
    pub successful_rate: f64,
}

/// 币种交易统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetStats {
    pub asset: String,
    pub trades: i64,
    pub profit: Decimal,
    pub volume: Decimal,
    pub avg_profit: Decimal,
}

impl DatabaseManager {
    /// 创建新的数据库管理器
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = MySqlPool::connect(database_url)
            .await
            .context("无法连接到MySQL数据库")?;
            
        let db_manager = Self {
            pool: Arc::new(pool),
            last_flush: Arc::new(Mutex::new(Instant::now())),
        };
        
        info!("数据库连接初始化完成");
        
        Ok(db_manager)
    }
    
    /// 记录套利结果
    pub async fn record_arbitrage_result(&self, result: &ArbitrageResult) -> Result<i64> {
        let duration_ms = (result.end_time - result.start_time).num_milliseconds() as i64;
        
        // 插入交易历史
        let id = sqlx::query!(
            r#"
            INSERT INTO arbitrage_history 
            (base_asset, buy_quote, sell_quote, buy_price, sell_price, 
             trade_amount, profit, profit_percentage, buy_order_id, sell_order_id,
             status, start_time, end_time, duration_ms)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            result.base_asset,
            result.buy_quote,
            result.sell_quote,
            result.buy_price.to_string(),
            result.sell_price.to_string(),
            result.trade_amount.to_string(),
            result.profit.to_string(),
            result.profit_percentage.to_string(),
            result.buy_order_id.map(|id| id as i64),
            result.sell_order_id.map(|id| id as i64),
            format!("{:?}", result.status),
            result.start_time.naive_utc(),
            result.end_time.naive_utc(),
            duration_ms
        )
        .execute(&*self.pool)
        .await?
        .last_insert_id() as i64;
        
        // 更新每日统计
        let date = result.start_time.format("%Y-%m-%d").to_string();
        let is_successful = matches!(result.status, ArbitrageStatus::Completed);
        
        sqlx::query!(
            r#"
            INSERT INTO daily_stats (date, trades, successful_trades, failed_trades, total_profit, total_volume)
            VALUES (?, 1, ?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE
                trades = trades + 1,
                successful_trades = successful_trades + ?,
                failed_trades = failed_trades + ?,
                total_profit = total_profit + ?,
                total_volume = total_volume + ?
            "#,
            date,
            if is_successful { 1 } else { 0 },
            if is_successful { 0 } else { 1 },
            result.profit.to_string(),
            result.trade_amount.to_string(),
            if is_successful { 1 } else { 0 },
            if is_successful { 0 } else { 1 },
            result.profit.to_string(),
            result.trade_amount.to_string()
        )
        .execute(&*self.pool)
        .await?;
        
        // 更新币种统计
        sqlx::query!(
            r#"
            INSERT INTO asset_stats (asset, trades, successful_trades, failed_trades, total_profit, total_volume)
            VALUES (?, 1, ?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE
                trades = trades + 1,
                successful_trades = successful_trades + ?,
                failed_trades = failed_trades + ?,
                total_profit = total_profit + ?,
                total_volume = total_volume + ?
            "#,
            result.base_asset,
            if is_successful { 1 } else { 0 },
            if is_successful { 0 } else { 1 },
            result.profit.to_string(),
            result.trade_amount.to_string(),
            if is_successful { 1 } else { 0 },
            if is_successful { 0 } else { 1 },
            result.profit.to_string(),
            result.trade_amount.to_string()
        )
        .execute(&*self.pool)
        .await?;
        
        debug!("记录套利结果: ID={}, 资产={}, 利润={}", id, result.base_asset, result.profit);
        
        Ok(id)
    }
    
    /// 获取总体交易统计
    pub async fn get_overall_stats(&self) -> Result<TradeStats> {
        let result = sqlx::query!(
            r#"
            SELECT
                COUNT(*) as total_trades,
                SUM(IF(status = 'Completed', 1, 0)) as successful_trades,
                SUM(IF(status != 'Completed', 1, 0)) as failed_trades,
                SUM(profit) as total_profit,
                SUM(trade_amount) as total_volume,
                AVG(profit) as avg_profit,
                MAX(profit) as max_profit,
                MIN(profit) as min_profit,
                AVG(duration_ms) as avg_duration
            FROM arbitrage_history
            "#
        )
        .fetch_one(&*self.pool)
        .await?;
        
        let total_trades = result.total_trades.unwrap_or(0);
        let successful_trades = result.successful_trades.unwrap_or(0);
        let failed_trades = result.failed_trades.unwrap_or(0);
        
        let total_profit = result.total_profit.map(|s| s.parse::<Decimal>().unwrap_or_default()).unwrap_or_default();
        let total_volume = result.total_volume.map(|s| s.parse::<Decimal>().unwrap_or_default()).unwrap_or_default();
        let avg_profit = result.avg_profit.map(|s| s.parse::<Decimal>().unwrap_or_default()).unwrap_or_default();
        let max_profit = result.max_profit.map(|s| s.parse::<Decimal>().unwrap_or_default()).unwrap_or_default();
        let min_profit = result.min_profit.map(|s| s.parse::<Decimal>().unwrap_or_default()).unwrap_or_default();
        
        let stats = TradeStats {
            total_trades,
            successful_trades,
            failed_trades,
            total_profit,
            total_volume,
            avg_profit_per_trade: avg_profit,
            max_profit,
            max_loss: min_profit,
            avg_trade_duration_ms: result.avg_duration.unwrap_or(0),
        };
        
        Ok(stats)
    }
    
    /// 获取每日交易统计
    pub async fn get_daily_stats(&self, days: i32) -> Result<Vec<DailyStats>> {
        let result = sqlx::query!(
            r#"
            SELECT
                date,
                trades,
                successful_trades,
                total_profit,
                total_volume
            FROM daily_stats
            WHERE date >= DATE_SUB(CURDATE(), INTERVAL ? DAY)
            ORDER BY date
            "#,
            days
        )
        .fetch_all(&*self.pool)
        .await?;
        
        let mut stats = Vec::new();
        
        for row in result {
            let date = row.date.format("%Y-%m-%d").to_string();
            let trades = row.trades as i64;
            let successful_trades = row.successful_trades as i64;
            let successful_rate = if trades > 0 {
                successful_trades as f64 / trades as f64 * 100.0
            } else {
                0.0
            };
            
            let profit = row.total_profit.parse::<Decimal>().unwrap_or_default();
            let volume = row.total_volume.parse::<Decimal>().unwrap_or_default();
            
            stats.push(DailyStats {
                date,
                trades,
                profit,
                volume,
                successful_rate,
            });
        }
        
        Ok(stats)
    }
    
    /// 获取币种交易统计
    pub async fn get_asset_stats(&self, limit: i32) -> Result<Vec<AssetStats>> {
        let result = sqlx::query!(
            r#"
            SELECT
                asset,
                trades,
                total_profit,
                total_volume
            FROM asset_stats
            ORDER BY total_profit DESC
            LIMIT ?
            "#,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;
        
        let mut stats = Vec::new();
        
        for row in result {
            let trades = row.trades as i64;
            let profit = row.total_profit.parse::<Decimal>().unwrap_or_default();
            let volume = row.total_volume.parse::<Decimal>().unwrap_or_default();
            let avg_profit = if trades > 0 {
                profit / Decimal::from(trades)
            } else {
                Decimal::default()
            };
            
            stats.push(AssetStats {
                asset: row.asset,
                trades,
                profit,
                volume,
                avg_profit,
            });
        }
        
        Ok(stats)
    }
    
    /// 查询历史交易记录
    pub async fn get_trade_history(
        &self,
        asset: Option<&str>,
        status: Option<ArbitrageStatus>,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<ArbitrageResult>> {
        let mut query = "
            SELECT
                id, base_asset, buy_quote, sell_quote,
                buy_price, sell_price, trade_amount, profit,
                profit_percentage, buy_order_id, sell_order_id,
                status, start_time, end_time
            FROM arbitrage_history
            WHERE 1=1
        ".to_string();
        
        let mut params = Vec::new();
        
        if let Some(asset_filter) = asset {
            query.push_str(" AND base_asset = ?");
            params.push(asset_filter.to_string());
        }
        
        if let Some(status_filter) = status {
            query.push_str(" AND status = ?");
            params.push(format!("{:?}", status_filter));
        }
        
        if let Some(start) = start_date {
            query.push_str(" AND start_time >= ?");
            params.push(start.format("%Y-%m-%d %H:%M:%S").to_string());
        }
        
        if let Some(end) = end_date {
            query.push_str(" AND start_time <= ?");
            params.push(end.format("%Y-%m-%d %H:%M:%S").to_string());
        }
        
        query.push_str(" ORDER BY start_time DESC LIMIT ? OFFSET ?");
        params.push(limit.to_string());
        params.push(offset.to_string());
        
        let query = sqlx::query(&query);
        let mut query = query;
        for param in params {
            query = query.bind(param);
        }
        
        let rows = query.fetch_all(&*self.pool).await?;
        
        let mut results = Vec::new();
        
        for row in rows {
            let id: i64 = row.get("id");
            let base_asset: String = row.get("base_asset");
            let buy_quote: String = row.get("buy_quote");
            let sell_quote: String = row.get("sell_quote");
            
            let buy_price: String = row.get("buy_price");
            let buy_price = buy_price.parse::<Decimal>().unwrap_or_default();
            
            let sell_price: String = row.get("sell_price");
            let sell_price = sell_price.parse::<Decimal>().unwrap_or_default();
            
            let trade_amount: String = row.get("trade_amount");
            let trade_amount = trade_amount.parse::<Decimal>().unwrap_or_default();
            
            let profit: String = row.get("profit");
            let profit = profit.parse::<Decimal>().unwrap_or_default();
            
            let profit_percentage: String = row.get("profit_percentage");
            let profit_percentage = profit_percentage.parse::<Decimal>().unwrap_or_default();
            
            let buy_order_id: Option<i64> = row.get("buy_order_id");
            let sell_order_id: Option<i64> = row.get("sell_order_id");
            
            let status: String = row.get("status");
            let status = match status.as_str() {
                "Identified" => ArbitrageStatus::Identified,
                "BuyOrderPlaced" => ArbitrageStatus::BuyOrderPlaced,
                "BuyOrderFilled" => ArbitrageStatus::BuyOrderFilled,
                "SellOrderPlaced" => ArbitrageStatus::SellOrderPlaced,
                "SellOrderFilled" => ArbitrageStatus::SellOrderFilled,
                "Completed" => ArbitrageStatus::Completed,
                "Failed" => ArbitrageStatus::Failed,
                _ => ArbitrageStatus::Failed,
            };
            
            let start_time: NaiveDateTime = row.get("start_time");
            let start_time = Utc.from_utc_datetime(&start_time);
            
            results.push(ArbitrageResult {
                base_asset,
                buy_quote,
                sell_quote,
                buy_price,
                sell_price,
                trade_amount,
                profit,
                profit_percentage,
                buy_order_id: buy_order_id.map(|id| id as u64),
                sell_order_id: sell_order_id.map(|id| id as u64),
                status,
                timestamp: start_time,
            });
        }
        
        Ok(results)
    }
}

// 模块测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ArbitrageStatus};
    use rust_decimal_macros::dec;
    
    // 这些测试需要有一个可用的MySQL数据库
    // 可以在测试时通过环境变量设置数据库连接字符串
    async fn get_test_db() -> DatabaseManager {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "mysql://user:password@localhost:3306/arbitrage_test".to_string());
        
        DatabaseManager::new(&database_url).await.expect("创建测试数据库管理器失败")
    }
    
    #[tokio::test]
    async fn test_record_arbitrage_result() {
        let db = get_test_db().await;
        
        let result = ArbitrageResult {
            base_asset: "BTC".to_string(),
            buy_quote: "USDT".to_string(),
            sell_quote: "USDC".to_string(),
            buy_price: dec!(50000),
            sell_price: dec!(50100),
            trade_amount: dec!(0.1),
            profit: dec!(10),
            profit_percentage: dec!(0.2),
            buy_order_id: Some(1),
            sell_order_id: Some(2),
            status: ArbitrageStatus::Completed,
            timestamp: Utc::now(),
            start_time: Utc::now(),
            end_time: Utc::now(),
        };
        let id = db.record_arbitrage_result(&result).await.expect("记录套利结果失败");
        assert!(id > 0);
    }
}
