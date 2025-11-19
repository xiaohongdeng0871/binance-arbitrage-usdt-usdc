//! 数据库模块，负责与MySQL交互并提供套利历史和绩效数据的存储与检索

use crate::models::{ArbitrageResult, ArbitrageStatus};
use anyhow::Result;
use chrono::{DateTime, Utc};
use log::info;
use rust_decimal::Decimal;
use sqlx::{MySql, Pool, Row};
use serde::{Deserialize, Serialize};

/// 数据库管理器，用于记录套利结果和统计数据
pub struct DatabaseManager {
    pool: Pool<MySql>,
}

impl DatabaseManager {
    /// 创建新的数据库管理器
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    /// 记录套利结果
    pub async fn record_arbitrage_result(&self, result: &ArbitrageResult) -> Result<u64> {
        let end_time = result.end_time.unwrap_or_else(Utc::now);
        let duration_ms = (end_time - result.start_time).num_milliseconds();
        
        let query = r#"
            INSERT INTO arbitrage_history (
                base_asset, buy_quote, sell_quote, buy_price, sell_price, 
                trade_amount, profit, profit_percentage, buy_order_id, sell_order_id,
                buy_funding_rate, sell_funding_rate, status, start_time, end_time, duration_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let status_str = match result.status {
            ArbitrageStatus::Identified => "Identified",
            ArbitrageStatus::Executing => "Executing",
            ArbitrageStatus::BuyOrderPlaced => "BuyOrderPlaced",
            ArbitrageStatus::BuyOrderFilled => "BuyOrderFilled",
            ArbitrageStatus::SellOrderPlaced => "SellOrderPlaced",
            ArbitrageStatus::SellOrderFilled => "SellOrderFilled",
            ArbitrageStatus::Completed => "Completed",
            ArbitrageStatus::Failed => "Failed",
        };

        let row = sqlx::query(query)
            .bind(&result.base_asset)
            .bind(&result.buy_quote)
            .bind(&result.sell_quote)
            .bind(result.buy_price)
            .bind(result.sell_price)
            .bind(result.trade_amount)
            .bind(result.profit)
            .bind(result.profit_percentage)
            .bind(result.buy_order_id)
            .bind(result.sell_order_id)
            .bind(result.buy_funding_rate)
            .bind(result.sell_funding_rate)
            .bind(status_str)
            .bind(result.start_time)
            .bind(end_time)
            .bind(duration_ms)
            .execute(&self.pool)
            .await?;

        let id = row.last_insert_id();
        info!("记录套利结果到数据库: ID={}", id);
        Ok(id)
    }
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
    /// 获取总体交易统计
    pub async fn get_overall_stats(&self) -> Result<TradeStats> {
        let result = sqlx::query!(
            r#"
            SELECT
                COUNT(*) as total_trades,
                SUM(IF(status = 'Completed', 1, 0)) as "successful_trades!: i64",
                SUM(IF(status != 'Completed', 1, 0)) as "failed_trades!: i64",
                SUM(profit) as total_profit,
                SUM(trade_amount) as total_volume,
                AVG(profit) as avg_profit,
                MAX(profit) as max_profit,
                MIN(profit) as min_profit,
                AVG(duration_ms) as "avg_duration!: i64"
            FROM arbitrage_history
            "#
        )
        .fetch_one(&self.pool)
        .await?;

        let stats = TradeStats {
            total_trades: result.total_trades,
            successful_trades: result.successful_trades,
            failed_trades: result.failed_trades,
            total_profit: result.total_profit.unwrap_or_default(),
            total_volume: result.total_volume.unwrap_or_default(),
            avg_profit_per_trade: result.avg_profit.unwrap_or_default(),
            max_profit: result.max_profit.unwrap_or_default(),
            max_loss: result.min_profit.unwrap_or_default(),
            avg_trade_duration_ms: result.avg_duration,
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
        .fetch_all(&self.pool)
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

            let profit = row.total_profit;
            let volume = row.total_volume;
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
                profit,
                volume
            FROM asset_stats
            ORDER BY profit DESC
            LIMIT ?
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut stats = Vec::new();
        
        for row in result {
            let trades = row.trades as i64;
            let profit = row.profit;
            let volume = row.volume;
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
    /// 注意：此方法当前未被使用
    #[allow(dead_code)]
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
        
        let rows = query.fetch_all(&self.pool).await?;
        
        let mut results = Vec::new();
        
        for row in rows {
            let status_str: String = row.get("status");
            let status = match status_str.as_str() {
                "Identified" => ArbitrageStatus::Identified,
                "Executing" => ArbitrageStatus::Executing,
                "BuyOrderPlaced" => ArbitrageStatus::BuyOrderPlaced,
                "BuyOrderFilled" => ArbitrageStatus::BuyOrderFilled,
                "SellOrderPlaced" => ArbitrageStatus::SellOrderPlaced,
                "SellOrderFilled" => ArbitrageStatus::SellOrderFilled,
                "Completed" => ArbitrageStatus::Completed,
                "Failed" => ArbitrageStatus::Failed,
                _ => ArbitrageStatus::Failed,
            };

            results.push(ArbitrageResult {
                base_asset: row.get("base_asset"),
                buy_quote: row.get("buy_quote"),
                sell_quote: row.get("sell_quote"),
                buy_price: row.get("buy_price"),
                sell_price: row.get("sell_price"),
                trade_amount: row.get("trade_amount"),
                profit: row.get("profit"),
                profit_percentage: row.get("profit_percentage"),
                buy_order_id: row.get("buy_order_id"),
                sell_order_id: row.get("sell_order_id"),
                status: status,
                start_time: row.get("start_time"),
                end_time: Some(row.get("end_time")),
                buy_funding_rate: row.get("buy_funding_rate"),
                sell_funding_rate: row.get("sell_funding_rate"),
            });

        }
        
        Ok(results)
    }
}

// 模块测试
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::mysql::MySqlPoolOptions;

    // 这些测试需要有一个可用的MySQL数据库
    // 可以在测试时通过环境变量设置数据库连接字符串
    #[allow(dead_code)]
    async fn get_test_db() -> DatabaseManager {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "mysql://user:password@localhost:3306/arbitrage_test".to_string());
        
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to create database pool");
        
        DatabaseManager::new(pool)
    }
    
    #[tokio::test]
    #[ignore] // 忽略测试，因为需要实际的数据库连接
    async fn test_record_arbitrage_result() {
        // 由于需要数据库连接，这个测试被忽略
        // 可以通过设置TEST_DATABASE_URL环境变量并运行:
        // cargo test test_record_arbitrage_result -- --ignored
        // 来执行这个测试
    }
}