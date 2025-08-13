use super::RiskController;
use crate::models::{ArbitrageOpportunity, ArbitrageResult, ArbitrageStatus, QuoteCurrency};
use crate::binance::ExchangeApi;
use anyhow::Result;
use async_trait::async_trait;
use log::{debug, info, warn};
use rust_decimal::{Decimal,dec};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use rust_decimal::prelude::Zero;

/// 风险敞口控制器
/// 控制单一币种的风险敞口，避免在特定币种上持有过多资产
pub struct ExposureController<T: ExchangeApi + Send + Sync> {
    api: Arc<T>,
    /// 币种最大风险敞口（以USDT计）
    max_exposures: HashMap<String, Decimal>,
    /// 每种币的当前头寸
    current_positions: Arc<Mutex<HashMap<String, Decimal>>>,
}

impl<T: ExchangeApi + Send + Sync + 'static> ExposureController<T> {
    pub fn new(api: Arc<T>) -> Self {
        Self {
            api,
            max_exposures: HashMap::new(),
            current_positions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// 设置币种最大风险敞口
    pub fn set_max_exposure(&mut self, asset: &str, max_exposure: Decimal) {
        self.max_exposures.insert(asset.to_string(), max_exposure);
        info!("设置 {} 最大风险敞口: {}", asset, max_exposure);
    }
    
    /// 更新当前持仓
    pub async fn update_positions(&self) -> Result<()> {

        for (asset, _) in &self.max_exposures {
            let balance = self.api.get_account_balance(asset).await?;
            {
                let mut positions = self.current_positions.lock().unwrap();
                positions.insert(asset.clone(), balance);
                debug!("更新持仓: {} = {}", asset, balance);
            }
        }
        
        Ok(())
    }
    
    /// 检查交易后的风险敞口是否超过限制
    fn check_exposure_after_trade(&self, asset: &str, change: Decimal) -> Result<(bool, Option<String>)> {
        let positions = self.current_positions.lock().unwrap();
        
        // 获取当前头寸
        let current_position = positions.get(asset).cloned().unwrap_or_else(Decimal::zero);
        
        // 交易后的头寸
        let new_position = current_position + change;
        
        // 检查是否有风险限制
        if let Some(max_exposure) = self.max_exposures.get(asset) {
            if new_position.abs() > *max_exposure {
                let reason = format!(
                    "{} 风险敞口将超过限制: {} + {} = {} > {}",
                    asset, current_position, change, new_position, max_exposure
                );
                warn!("{}", reason);
                return Ok((false, Some(reason)));
            }
        }
        
        Ok((true, None))
    }
}

#[async_trait]
impl<T: ExchangeApi + Send + Sync + 'static> RiskController for ExposureController<T> {
    fn name(&self) -> &str {
        "风险敞口控制"
    }
    
    fn description(&self) -> &str {
        "控制单一币种的风险敞口，避免在特定币种上持有过多资产"
    }
    
    async fn check_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<(bool, Option<String>)> {
        // 更新当前持仓
        self.update_positions().await?;
        
        // 计算交易对基础资产的变化
        // 套利交易通常是先买入后卖出，基础资产的净变化应该是很小的
        // 但我们仍然检查以防万一
        
        let base_asset = &opportunity.base_asset;
        let trade_amount_base = opportunity.max_trade_amount / opportunity.buy_price;
        
        // 检查基础资产的风险敞口
        self.check_exposure_after_trade(base_asset, trade_amount_base)
    }
    
    async fn record_result(&self, result: &ArbitrageResult) -> Result<()> {
        if result.status == ArbitrageStatus::Completed {
            let mut positions = self.current_positions.lock().unwrap();
            
            // 更新基础资产头寸（买入后卖出，净变化应该很小，但仍然要记录）
            let _ = positions.entry(result.base_asset.clone()).or_insert(Decimal::ZERO);
            
            // 这里假设交易已经完成，资产头寸已经反映在账户余额中
            // 实际上应该再次调用API获取最新头寸，但这里为了简化，我们只是记录交易
            info!(
                "套利交易完成: {} - 利润: {}",
                result.base_asset, result.profit
            );
        }
        
        Ok(())
    }
    
    async fn reset(&self) -> Result<()> {
        let mut positions = self.current_positions.lock().unwrap();
        positions.clear();
        
        info!("重置风险敞口控制器");
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binance::MockBinanceApi;
    
    #[tokio::test]
    async fn test_exposure_control() {
        let api = MockBinanceApi::new();
        let mut controller = ExposureController::new(api);
        
        // 设置BTC的最大风险敞口为2个BTC
        controller.set_max_exposure("BTC", dec!(2));
        
        // 模拟更新持仓
        {
            let mut positions = controller.current_positions.lock().unwrap();
            positions.insert("BTC".to_string(), dec!(1.5));
        }
        
        // 创建一个会超过风险敞口的套利机会
        let opportunity = ArbitrageOpportunity::new(
            "BTC",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(50000),
            dec!(50100),
            dec!(50000),  // 交易1 BTC
        );
        
        // 应该被拒绝
        let (valid, reason) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(!valid);
        assert!(reason.unwrap().contains("风险敞口将超过限制"));
        
        // 创建一个不会超过风险敞口的套利机会
        let opportunity = ArbitrageOpportunity::new(
            "BTC",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(50000),
            dec!(50100),
            dec!(10000),  // 交易0.2 BTC
        );
        
        // 应该通过
        let (valid, _) = controller.check_opportunity(&opportunity).await.unwrap();
        assert!(valid);
    }
}
