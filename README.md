# 币安 USDT/USDC 套利程序

这是一个用Rust编写的自动化套利程序，用于在币安交易所上对USDT和USDC交易对之间的价格差异进行套利。程序能够监控价格差异，并在满足设定的利润阈值时自动执行交易。

## 功能特点

- 实时监控币安交易所的USDT和USDC交易对价格差异
- 支持自定义套利参数（最小利润百分比、最大交易金额、检查间隔）
- 提供真实交易模式和模拟交易模式
- 自动执行买入和卖出订单，实现套利
- 详细的日志记录和错误处理
- 支持多种基础资产（BTC、ETH等）
- **多种交易策略**：简单价格差异、TWAP、订单簿深度分析、滑点控制和趋势跟踪
- **完善的风控机制**：每日亏损限制、异常价格保护、风险敞口控制、交易时间窗口、交易频率控制和交易对黑名单
- **套利历史记录**：将所有套利交易记录保存到MySQL数据库
- **绩效分析**：生成详细的绩效报告，包括收益统计、成功率分析和币种表现

## 安装要求

- Rust 1.60+
- Cargo 1.60+
- 有效的币安API密钥（真实交易模式）
- MySQL数据库（用于存储套利历史记录和分析）

## 安装步骤

1. 克隆仓库：

bash
git clone https://github.com/your-repo/binance-arbitrage.git
cd binance-arbitrage

2. 创建并配置环境变量文件：

bash
cp .env.example .env
```
然后编辑`.env`文件，添加你的币安API密钥和其他必要的配置。
3. 创建MySQL数据库：

CREATE DATABASE arbitrage;
4. 构建程序：

bash
cargo build --release
## 使用方法

### 命令行参数

程序提供了以下命令行参数：

bash
./target/release/binance_arbitrage -h

### 实时交易模式

使用真实的币安API执行交易：

bash
./target/release/binance_arbitrage -b BTC --min-profit 0.2 --max-amount 100 --interval 1000

参数说明：
- `-b BTC`: 使用BTC作为基础资产
- `--min-profit 0.2`: 最小利润百分比为0.2%
- `--max-amount 100`: 最大交易金额为100 USDT
- `--interval 1000`: 价格检查间隔为1000毫秒（1秒）

### 模拟交易模式

使用模拟数据测试套利逻辑：

bash
./target/release/binance_arbitrage --simulate -b ETH --min-profit 0.1 --max-amount 200 --interval 500 --runtime 300

参数说明：
- `-b ETH`: 使用ETH作为基础资产
- `--min-profit 0.1`: 最小利润百分比为0.1%
- `--max-amount 200`: 最大交易金额为200 USDT
- `--interval 500`: 价格检查间隔为500毫秒（0.5秒）
- `--runtime 300`: 模拟程序运行300秒（5分钟）后自动停止
- `--volatility 2.0`: 价格波动率为2%
- `--opportunity-probability 50`: 50%的概率创建套利机会

### 绩效分析

分析历史套利数据并生成报告：

bash
./target/release/binance_arbitrage --analyze --time-range today --export-format json --export-path ./report.json
参数说明：
- `--time-range`: 分析时间范围，可选值: today, yesterday, last7days, last30days, thismonth, lastmonth, alltime, custom
- `--export-format`: 导出格式，可选值: json, csv
- `--export-path`: 报告导出路径
- `--start-date` 和 `--end-date`: 自定义时间范围的开始和结束日期（YYYY-MM-DD格式）
- `--top-assets`: 显示表现最好的前N个币种
## 多种交易策略

程序支持以下交易策略：

- **simple**: 简单价格差异套利 - 基于USDT和USDC交易对之间的直接价格差异
- **twap**: 时间加权平均价格策略 - 将大订单分解为小订单在一段时间内执行
- **depth**: 订单簿深度分析 - 考虑订单簿深度和流动性进行交易决策
- **slippage**: 滑点控制策略 - 控制成交价格滑点，避免在波动大的市场中亏损
- **trend**: 趋势跟踪策略 - 结合短期价格趋势，避免在价格快速变化时进行套利

## 风控机制

程序实现了以下风险控制机制：

- **loss-limit**: 每日亏损限制 - 设置每日最大亏损额度，超过后停止交易
- **abnormal-price**: 异常价格检测 - 检测极端价格波动，暂停交易
- **exposure**: 风险敞口控制 - 控制单一币种的风险敞口
- **time-window**: 交易时间限制 - 只在特定时间段内交易
- **frequency**: 交易频率限制 - 控制套利交易的频率，避免API限制
- **blacklist**: 交易对黑名单 - 将某些交易对列入黑名单，不参与套利

## 套利历史记录和绩效分析

程序可以将所有套利交易记录保存到MySQL数据库，并支持生成详细的绩效分析报告：

- **总体统计**: 总交易次数、成功率、总利润、盈亏比等
- **每日统计**: 按日期统计交易量和利润
- **币种表现**: 分析不同币种的表现和收益情况
- **导出格式**: 支持导出为JSON和CSV格式
- **时间范围**: 支持多种预设时间范围和自定义日期范围

## 数据库模式

程序使用以下数据库表结构：

1. `arbitrage_history`: 存储所有套利交易的详细记录
2. `daily_stats`: 按日汇总的交易统计数据
3. `asset_stats`: 按币种统计的交易数据

### 数据库表结构

#### 套利历史记录表 (arbitrage_history)

sql
CREATE TABLE arbitrage_history (
    id INT AUTO_INCREMENT PRIMARY KEY,
    base_asset VARCHAR(10) NOT NULL,
    quote_asset VARCHAR(10) NOT NULL,
    buy_price DECIMAL(18, 8) NOT NULL,
    sell_price DECIMAL(18, 8) NOT NULL,
    amount DECIMAL(18, 8) NOT NULL,
    profit DECIMAL(18, 8) NOT NULL,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
#### 每日统计表 (daily_stats)

CREATE TABLE daily_stats (
    id INT AUTO_INCREMENT PRIMARY KEY,
    date DATE NOT NULL,
    total_trades INT NOT NULL,
    successful_trades INT NOT NULL,
    total_profit DECIMAL(18, 8) NOT NULL,
    win_rate DECIMAL(5, 2) NOT NULL,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);

#### 币种统计表 (asset_stats)

CREATE TABLE asset_stats (
    id INT AUTO_INCREMENT PRIMARY KEY,
    asset VARCHAR(10) NOT NULL,
    total_trades INT NOT NULL,
    successful_trades INT NOT NULL,
    total_profit DECIMAL(18, 8) NOT NULL,
    win_rate DECIMAL(5, 2) NOT NULL,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);

## 配置文件

`.env`文件配置示例：

plaintext
BINANCE_API_KEY=your_api_key_here
BINANCE_API_SECRET=your_api_secret_here
BASE_ASSET=BTC
MIN_PROFIT=0.2
MAX_AMOUNT=100
CHECK_INTERVAL=1000
DB_URL=mysql://user:password@localhost/arbitrage
## 安全注意事项

- 请妥善保管你的API密钥，不要将其提交到版本控制系统或公开分享
- 建议先使用模拟模式测试程序，确保一切正常后再使用实时模式
- 在币安API账户中设置交易限制，以防止意外的大额交易
- 定期检查日志和绩效报告，监控套利性能和任何潜在问题
- 定期备份数据库，以防止数据丢失

## 许可证

MIT License
