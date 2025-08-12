use super::RiskController;
use crate::models::{ArbitrageOpportunity, ArbitrageResult};
use anyhow::Result;
use async_trait::async_trait;
use log::{debug, info, warn};
use std::sync::{Arc, Mutex};
use std::collections::HashSet;

/// 交易对黑名单控制器
/// 将特定交易对排除在套利操作之外，可用于避免问题币种或特定市场情况
pub struct PairBlacklistController {
    /// 黑名单交易对集合
    blacklist: Arc<Mutex<HashSet<String>>>,
}

impl PairBlacklistController {
    pub fn new() -> Self {
        Self {
            blacklist: Arc::new(Mutex::new(HashSet::new())),
        }
    }
    
    /// 添加交易对到黑名单
    pub fn add_to_blacklist(&self, asset: &str, quote: &str) {
        let pair = format!("{}{}", asset, quote);
        let mut blacklist = self.blacklist.lock().unwrap();
        blacklist.insert(pair.clone());
        info!("添加交易对到黑名单: {}", pair);
    }
    
    /// 添加一组基础资产到黑名单
    pub fn add_base_asset_to_blacklist(&self, base_asset: &str) {
        let usdt_pair = format!("{}USDT", base_asset);
        let usdc_pair = format!("{}USDC", base_asset);
        
        let mut blacklist = self.blacklist.lock().unwrap();
        blacklist.insert(usdt_pair.clone());
        blacklist.insert(usdc_pair.clone());
        
        info!("添加基础资产到黑名单: {} (添加 {} 和 {})", base_asset, usdt_pair, usdc_pair);
    }
    
    /// 从黑名单中移除交易对
    pub fn remove_from_blacklist(&self, asset: &str, quote: &str) {
        let pair = format!("{}{}", asset, quote);
        let mut blacklist = self.blacklist.lock().unwrap();
        if blacklist.remove(&pair) {
            info!("从黑名单移除交易对: {}", pair);
        }
    }
    
    /// 从黑名单中移除一组基础资产
    pub fn remove_base_asset_from_blacklist(&self, base_asset: &str) {
        let usdt_pair = format!("{}USDT", base_asset);
        let usdc_pair = format!("{}USDC", base_asset);
        
        let mut blacklist = self.blacklist.lock().unwrap();
        let removed_usdt = blacklist.remove(&usdt_pair);
        let removed_usdc = blacklist.remove(&usdc_pair);
        
        if removed_usdt || removed_usdc {
            info!("从黑名单移除基础资产: {}", base_asset);
        }
    }
    
    /// 检查交易对是否在黑名单中
    fn is_blacklisted(&self, asset: &str, quote: &str) -> bool {
        let pair = format!("{}{}", asset, quote);
        let blacklist = self.blacklist.lock().unwrap();
        blacklist.contains(&pair)
    }
    
    /// 获取所有黑名单交易对
    pub fn get_blacklist(&self) -> Vec<String> {
        let blacklist = self.blacklist.lock().unwrap();
        blacklist.iter().cloned().collect()
    }
}

#[async_trait]
impl RiskController for PairBlacklistController {
    fn name(&self) -> &str {
        "交易对黑名单"
    }
    
    fn description(&self) -> &str {
        "将特定交易对排除在套利操作之外，可用于避免问题币种或特定市场情况"
    }
    
    async fn check_opportunity(&self, opportunity: &ArbitrageOpportunity) -> Result<(bool, Option<String>)> {
        let usdt_pair = format!("{}USDT", opportunity.base_asset);
        let usdc_pair = format!("{}USDC", opportunity.base_asset);
        
        let blacklist = self.blacklist.lock().unwrap();
        
        // 检查两个交易对是否有任何一个在黑名单中
        if blacklist.contains(&usdt_pair) || blacklist.contains(&usdc_pair) {
            let reason = format!(
                "{} 在黑名单中，不执行套利",
                opportunity.base_asset
            );
            debug!("{}", reason);
            return Ok((false, Some(reason)));
        }
        
        Ok((true, None))
    }
    
    async fn record_result(&self, _result: &ArbitrageResult) -> Result<()> {
        // 这个控制器不需要记录交易结果
        Ok(())
    }
    
    async fn reset(&self) -> Result<()> {
        let mut blacklist = self.blacklist.lock().unwrap();
        blacklist.clear();
        
        info!("重置交易对黑名单控制器");
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuoteCurrency};
    use rust_decimal::dec;
    
    #[tokio::test]
    async fn test_pair_blacklist() {
        let controller = PairBlacklistController::new();
        
        // 添加BTC到黑名单
        controller.add_base_asset_to_blacklist("BTC");
        
        // 创建一个BTC的套利机会
        let btc_opportunity = ArbitrageOpportunity::new(
            "BTC",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(50000),
            dec!(50100),
            dec!(1000),
        );
        
        // 应该被拒绝
        let (valid, reason) = controller.check_opportunity(&btc_opportunity).await.unwrap();
        assert!(!valid);
        assert!(reason.unwrap().contains("BTC 在黑名单中"));
        
        // 创建一个ETH的套利机会
        let eth_opportunity = ArbitrageOpportunity::new(
            "ETH",
            QuoteCurrency::USDT,
            QuoteCurrency::USDC,
            dec!(3000),
            dec!(3010),
            dec!(1000),
        );
        
        // 应该通过
        let (valid, _) = controller.check_opportunity(&eth_opportunity).await.unwrap();
        assert!(valid);
        
        // 从黑名单中移除BTC
        controller.remove_base_asset_from_blacklist("BTC");
        
        // 现在BTC应该能通过
        let (valid, _) = controller.check_opportunity(&btc_opportunity).await.unwrap();
        assert!(valid);
    }
}
