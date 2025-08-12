-- 套利历史表
CREATE TABLE `arbitrage_history` (
    `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
    `base_asset` VARCHAR(20) NOT NULL COMMENT '基础资产，如BTC',
    `buy_quote` VARCHAR(20) NOT NULL COMMENT '买入报价货币',
    `sell_quote` VARCHAR(20) NOT NULL COMMENT '卖出报价货币',
    `buy_price` DECIMAL(20, 8) NOT NULL COMMENT '买入价格',
    `sell_price` DECIMAL(20, 8) NOT NULL COMMENT '卖出价格',
    `trade_amount` DECIMAL(20, 8) NOT NULL COMMENT '交易数量',
    `profit` DECIMAL(20, 8) NOT NULL COMMENT '利润',
    `profit_percentage` DECIMAL(10, 4) NOT NULL COMMENT '利润率百分比',
    `buy_order_id` BIGINT UNSIGNED DEFAULT NULL COMMENT '买入订单ID',
    `sell_order_id` BIGINT UNSIGNED DEFAULT NULL COMMENT '卖出订单ID',
    `status` VARCHAR(20) NOT NULL COMMENT '套利状态',
    `start_time` DATETIME NOT NULL COMMENT '开始时间',
    `end_time` DATETIME NOT NULL COMMENT '结束时间',
    `duration_ms` BIGINT NOT NULL COMMENT '持续时间(毫秒)',
    `created_at` TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
    PRIMARY KEY (`id`),
    KEY `idx_base_asset` (`base_asset`),
    KEY `idx_status` (`status`),
    KEY `idx_created_at` (`created_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='套利历史表';

-- 每日统计表
CREATE TABLE `daily_stats` (
    `date` DATE NOT NULL COMMENT '日期',
    `trades` INT NOT NULL DEFAULT '0' COMMENT '交易次数',
    `successful_trades` INT NOT NULL DEFAULT '0' COMMENT '成功交易次数',
    `failed_trades` INT NOT NULL DEFAULT '0' COMMENT '失败交易次数',
    `total_profit` DECIMAL(20, 8) NOT NULL DEFAULT '0' COMMENT '总利润',
    `total_volume` DECIMAL(20, 8) NOT NULL DEFAULT '0' COMMENT '总交易量',
    PRIMARY KEY (`date`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='每日统计表';

-- 币种统计表
CREATE TABLE `asset_stats` (
    `asset` VARCHAR(20) NOT NULL COMMENT '资产名称',
    `trades` INT NOT NULL DEFAULT '0' COMMENT '交易次数',
    `successful_trades` INT NOT NULL DEFAULT '0' COMMENT '成功交易次数',
    `failed_trades` INT NOT NULL DEFAULT '0' COMMENT '失败交易次数',
    `total_profit` DECIMAL(20, 8) NOT NULL DEFAULT '0' COMMENT '总利润',
    `total_volume` DECIMAL(20, 8) NOT NULL DEFAULT '0' COMMENT '总交易量',
    PRIMARY KEY (`asset`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='币种统计表';