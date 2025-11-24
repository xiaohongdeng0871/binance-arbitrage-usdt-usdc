use crate::config::Config;
use crate::exchanges::exchange::{Exchange, ExchangeApi};
use crate::exchanges::{BinanceApi, OkxApi, GateIoApi, BitgetApi};
use anyhow::Result;

/// 交易所API工厂，用于创建指定交易所的API实例
pub struct ExchangeFactory;

impl ExchangeFactory {
    /// 根据交易所类型和配置创建对应的API实例
    /// 
    /// # 参数
    /// * `exchange` - 交易所类型
    /// * `config` - 配置信息
    /// 
    /// # 返回值
    /// 返回对应的交易所API实例
    pub fn create_exchange_api(exchange: Exchange, config: Config) -> Result<Box<dyn ExchangeApi>> {
        let api: Box<dyn ExchangeApi> = match exchange {
            Exchange::Binance => Box::new(BinanceApi::new(config)),
            Exchange::Okx => Box::new(OkxApi::new(config)),
            Exchange::GateIo => Box::new(GateIoApi::new(config)),
            Exchange::Bitget => Box::new(BitgetApi::new(config)),
            _ => return Err(anyhow::anyhow!("Unsupported exchange: {:?}", exchange)),
        };
        
        Ok(api)
    }
    
    /// 根据交易所名称和配置创建对应的API实例
    /// 
    /// # 参数
    /// * `exchange_name` - 交易所名称
    /// * `config` - 配置信息
    /// 
    /// # 返回值
    /// 返回对应的交易所API实例
    pub fn create_exchange_api_by_name(exchange_name: &str, config: Config) -> Result<Box<dyn ExchangeApi>> {
        let exchange = Exchange::from_str(exchange_name)
            .ok_or_else(|| anyhow::anyhow!("Unsupported exchange: {}", exchange_name))?;
        
        Self::create_exchange_api(exchange, config)
    }
}