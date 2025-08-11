CREATE TABLE `arbitrage_history` (
                                     `id` bigint(20) unsigned NOT NULL AUTO_INCREMENT COMMENT '主键ID',
                                     `base_asset` varchar(20) NOT NULL DEFAULT '' COMMENT '基础资产（如BTC、ETH等）',
                                     `buy_quote` varchar(20) NOT NULL DEFAULT '' COMMENT '买入计价货币（如USDT）',
                                     `sell_quote` varchar(20) NOT NULL DEFAULT '' COMMENT '卖出计价货币（如USDC）',
                                     `buy_price` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '买入价格',
                                     `sell_price` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '卖出价格',
                                     `trade_amount` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '交易数量',
                                     `profit` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '利润',
                                     `profit_percentage` decimal(10,4) NOT NULL DEFAULT '0.0000' COMMENT '利润率(%)',
                                     `buy_order_id` bigint(20) unsigned DEFAULT NULL COMMENT '买入订单ID',
                                     `sell_order_id` bigint(20) unsigned DEFAULT NULL COMMENT '卖出订单ID',
                                     `status` varchar(30) NOT NULL DEFAULT 'Failed' COMMENT '交易状态(Identified,BuyOrderPlaced,BuyOrderFilled,SellOrderPlaced,SellOrderFilled,Completed,Failed)',
                                     `start_time` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '开始时间',
                                     `end_time` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '结束时间',
                                     `duration_ms` bigint(20) NOT NULL DEFAULT '0' COMMENT '持续时间(毫秒)',
                                     `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
                                     `updated_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
                                     PRIMARY KEY (`id`),
                                     KEY `idx_base_asset` (`base_asset`),
                                     KEY `idx_status` (`status`),
                                     KEY `idx_start_time` (`start_time`),
                                     KEY `idx_created_at` (`created_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='套利交易历史表';

CREATE TABLE `daily_stats` (
                               `id` bigint(20) unsigned NOT NULL AUTO_INCREMENT COMMENT '主键ID',
                               `date` date NOT NULL COMMENT '日期',
                               `trades` int(11) NOT NULL DEFAULT '0' COMMENT '总交易次数',
                               `successful_trades` int(11) NOT NULL DEFAULT '0' COMMENT '成功交易次数',
                               `failed_trades` int(11) NOT NULL DEFAULT '0' COMMENT '失败交易次数',
                               `total_profit` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '总利润',
                               `total_volume` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '总交易量',
                               `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
                               `updated_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
                               PRIMARY KEY (`id`),
                               UNIQUE KEY `uk_date` (`date`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='每日交易统计表';

CREATE TABLE `asset_stats` (
                               `id` bigint(20) unsigned NOT NULL AUTO_INCREMENT COMMENT '主键ID',
                               `asset` varchar(20) NOT NULL COMMENT '资产名称',
                               `trades` int(11) NOT NULL DEFAULT '0' COMMENT '交易次数',
                               `profit` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '总利润',
                               `volume` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '总交易量',
                               `avg_profit` decimal(20,8) NOT NULL DEFAULT '0.00000000' COMMENT '平均利润',
                               `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
                               `updated_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
                               PRIMARY KEY (`id`),
                               UNIQUE KEY `uk_asset` (`asset`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='币种交易统计表';
