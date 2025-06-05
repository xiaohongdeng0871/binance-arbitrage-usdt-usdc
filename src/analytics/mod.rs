//! 套利绩效分析模块，提供数据分析和报告生成功能

use crate::db::{DatabaseManager, TradeStats, DailyStats, AssetStats};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc, Duration, Local, TimeZone, NaiveDate};
use log::{debug, info, warn, error};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Serialize, Deserialize};
use std::path::Path;
use std::fs::File;
use std::io::Write;
use std::collections::HashMap;
use csv::Writer as CsvWriter;

/// 分析时间范围
pub enum TimeRange {
    /// 今日数据
    Today,
    /// 昨日数据
    Yesterday,
    /// 过去7天
    Last7Days,
    /// 过去30天
    Last30Days,
    /// 本月数据
    ThisMonth,
    /// 上月数据
    LastMonth,
    /// 全部历史数据
    AllTime,
    /// 自定义时间范围
    Custom(DateTime<Utc>, DateTime<Utc>),
}

impl TimeRange {
    /// 获取时间范围的开始和结束时间
    pub fn get_date_range(&self) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        let now = Utc::now();
        let today = Local::now().date_naive();
        
        match self {
            TimeRange::Today => {
                let start = Local.from_local_date(&today).unwrap().and_hms_opt(0, 0, 0).unwrap().with_timezone(&Utc);
                (Some(start), None)
            },
            TimeRange::Yesterday => {
                let yesterday = today.pred_opt().unwrap();
                let start = Local.from_local_date(&yesterday).unwrap().and_hms_opt(0, 0, 0).unwrap().with_timezone(&Utc);
                let end = Local.from_local_date(&today).unwrap().and_hms_opt(0, 0, 0).unwrap().with_timezone(&Utc);
                (Some(start), Some(end))
            },
            TimeRange::Last7Days => {
                let start = now - Duration::days(7);
                (Some(start), None)
            },
            TimeRange::Last30Days => {
                let start = now - Duration::days(30);
                (Some(start), None)
            },
            TimeRange::ThisMonth => {
                let current_month = today.month();
                let current_year = today.year();
                let first_day = NaiveDate::from_ymd_opt(current_year, current_month, 1).unwrap();
                let start = Local.from_local_date(&first_day).unwrap().and_hms_opt(0, 0, 0).unwrap().with_timezone(&Utc);
                (Some(start), None)
            },
            TimeRange::LastMonth => {
                let current_month = today.month();
                let current_year = today.year();
                
                let (prev_year, prev_month) = if current_month == 1 {
                    (current_year - 1, 12)
                } else {
                    (current_year, current_month - 1)
                };
                
                let first_day_prev = NaiveDate::from_ymd_opt(prev_year, prev_month, 1).unwrap();
                let start = Local.from_local_date(&first_day_prev).unwrap().and_hms_opt(0, 0, 0).unwrap().with_timezone(&Utc);
                
                let first_day_current = NaiveDate::from_ymd_opt(current_year, current_month, 1).unwrap();
                let end = Local.from_local_date(&first_day_current).unwrap().and_hms_opt(0, 0, 0).unwrap().with_timezone(&Utc);
                
                (Some(start), Some(end))
            },
            TimeRange::AllTime => (None, None),
            TimeRange::Custom(start, end) => (Some(*start), Some(*end)),
        }
    }
    
    /// 获取时间范围的描述
    pub fn description(&self) -> String {
        match self {
            TimeRange::Today => "今日".to_string(),
            TimeRange::Yesterday => "昨日".to_string(),
            TimeRange::Last7Days => "过去7天".to_string(),
            TimeRange::Last30Days => "过去30天".to_string(),
            TimeRange::ThisMonth => "本月".to_string(),
            TimeRange::LastMonth => "上月".to_string(),
            TimeRange::AllTime => "全部历史".to_string(),
            TimeRange::Custom(start, end) => {
                format!("{}至{}", 
                    start.format("%Y-%m-%d"),
                    end.format("%Y-%m-%d")
                )
            },
        }
    }
}

/// 绩效报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    /// 报告标题
    pub title: String,
    /// 时间范围描述
    pub time_range: String,
    /// 生成时间
    pub generated_at: DateTime<Utc>,
    /// 总体统计
    pub overview: TradeStats,
    /// 每日统计
    pub daily_stats: Vec<DailyStats>,
    /// 币种统计
    pub asset_stats: Vec<AssetStats>,
    /// 成功率 (百分比)
    pub success_rate: f64,
    /// 盈亏比
    pub profit_loss_ratio: f64,
    /// 日均交易量
    pub avg_daily_volume: Decimal,
    /// 日均利润
    pub avg_daily_profit: Decimal,
    /// 最佳交易日
    pub best_day: Option<DailyStats>,
    /// 最差交易日
    pub worst_day: Option<DailyStats>,
}

/// 分析管理器
pub struct AnalyticsManager {
    db: DatabaseManager,
}

impl AnalyticsManager {
    /// 创建新的分析管理器
    pub fn new(db: DatabaseManager) -> Self {
        Self { db }
    }
    
    /// 生成绩效分析报告
    pub async fn generate_report(&self, range: TimeRange) -> Result<PerformanceReport> {
        let (start_date, end_date) = range.get_date_range();
        
        // 获取总体统计
        let overview = self.db.get_overall_stats().await?;
        
        // 计算每日统计
        let daily_length = match &range {
            TimeRange::Last7Days => 7,
            TimeRange::Last30Days => 30,
            TimeRange::ThisMonth | TimeRange::LastMonth => 31,
            _ => 30, // 默认显示30天数据
        };
        
        let daily_stats = self.db.get_daily_stats(daily_length).await?;
        
        // 获取币种统计
        let asset_stats = self.db.get_asset_stats(10).await?;
        
        // 计算成功率
        let success_rate = if overview.total_trades > 0 {
            (overview.successful_trades as f64 / overview.total_trades as f64) * 100.0
        } else {
            0.0
        };
        
        // 计算盈亏比 (平均盈利 / 平均亏损)
        let profit_loss_ratio = if overview.max_loss.abs() > dec!(0) {
            (overview.max_profit / overview.max_loss.abs()).to_f64().unwrap_or(0.0)
        } else {
            0.0
        };
        
        // 找出最佳和最差交易日
        let mut best_day = None;
        let mut worst_day = None;
        
        if !daily_stats.is_empty() {
            let mut max_profit = Decimal::MIN;
            let mut min_profit = Decimal::MAX;
            
            for stats in daily_stats.iter() {
                if stats.profit > max_profit {
                    max_profit = stats.profit;
                    best_day = Some(stats.clone());
                }
                
                if stats.profit < min_profit {
                    min_profit = stats.profit;
                    worst_day = Some(stats.clone());
                }
            }
        }
        
        // 计算日均交易量和利润
        let days_with_trades = daily_stats.iter().filter(|s| s.trades > 0).count();
        let avg_daily_volume = if days_with_trades > 0 {
            let total_volume: Decimal = daily_stats.iter().map(|s| s.volume).sum();
            total_volume / Decimal::from(days_with_trades)
        } else {
            Decimal::ZERO
        };
        
        let avg_daily_profit = if days_with_trades > 0 {
            let total_profit: Decimal = daily_stats.iter().map(|s| s.profit).sum();
            total_profit / Decimal::from(days_with_trades)
        } else {
            Decimal::ZERO
        };
        
        Ok(PerformanceReport {
            title: format!("套利交易绩效报告 - {}", range.description()),
            time_range: range.description(),
            generated_at: Utc::now(),
            overview,
            daily_stats,
            asset_stats,
            success_rate,
            profit_loss_ratio,
            avg_daily_volume,
            avg_daily_profit,
            best_day,
            worst_day,
        })
    }
    
    /// 将报告导出为CSV格式
    pub async fn export_report_to_csv(&self, report: &PerformanceReport, path: &Path) -> Result<()> {
        let mut daily_writer = CsvWriter::from_path(path.join("daily_stats.csv"))?;
        
        // 写入表头
        daily_writer.write_record(&["日期", "交易数量", "利润(USDT)", "交易量(USDT)", "成功率(%)"])?;
        
        // 写入每日数据
        for stats in &report.daily_stats {
            daily_writer.write_record(&[
                &stats.date,
                &stats.trades.to_string(),
                &stats.profit.to_string(),
                &stats.volume.to_string(),
                &format!("{:.2}", stats.successful_rate),
            ])?;
        }
        daily_writer.flush()?;
        
        // 写入币种统计
        let mut asset_writer = CsvWriter::from_path(path.join("asset_stats.csv"))?;
        asset_writer.write_record(&["币种", "交易数量", "总利润(USDT)", "总交易量(USDT)", "平均每笔利润(USDT)"])?;
        
        for stats in &report.asset_stats {
            asset_writer.write_record(&[
                &stats.asset,
                &stats.trades.to_string(),
                &stats.profit.to_string(),
                &stats.volume.to_string(),
                &stats.avg_profit.to_string(),
            ])?;
        }
        asset_writer.flush()?;
        
        // 写入总体统计
        let mut overview_writer = CsvWriter::from_path(path.join("overview.csv"))?;
        overview_writer.write_record(&["统计指标", "数值"])?;
        
        overview_writer.write_record(&["总交易次数", &report.overview.total_trades.to_string()])?;
        overview_writer.write_record(&["成功交易次数", &report.overview.successful_trades.to_string()])?;
        overview_writer.write_record(&["失败交易次数", &report.overview.failed_trades.to_string()])?;
        overview_writer.write_record(&["总利润(USDT)", &report.overview.total_profit.to_string()])?;
        overview_writer.write_record(&["总交易量(USDT)", &report.overview.total_volume.to_string()])?;
        overview_writer.write_record(&["平均每笔利润(USDT)", &report.overview.avg_profit_per_trade.to_string()])?;
        overview_writer.write_record(&["最大单笔利润(USDT)", &report.overview.max_profit.to_string()])?;
        overview_writer.write_record(&["最大单笔亏损(USDT)", &report.overview.max_loss.to_string()])?;
        overview_writer.write_record(&["成功率(%)", &format!("{:.2}", report.success_rate)])?;
        overview_writer.write_record(&["盈亏比", &format!("{:.2}", report.profit_loss_ratio)])?;
        overview_writer.write_record(&["平均每日交易量(USDT)", &report.avg_daily_volume.to_string()])?;
        overview_writer.write_record(&["平均每日利润(USDT)", &report.avg_daily_profit.to_string()])?;
        
        overview_writer.flush()?;
        
        info!("已将绩效报告导出为CSV格式: {:?}", path);
        
        Ok(())
    }
    
    /// 将报告保存为JSON格式
    pub async fn export_report_to_json(&self, report: &PerformanceReport, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(report)?;
        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;
        
        info!("已将绩效报告导出为JSON格式: {:?}", path);
        
        Ok(())
    }
}
