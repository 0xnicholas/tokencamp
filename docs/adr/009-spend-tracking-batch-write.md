# ADR-009: 成本追踪的批量写入模式

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp 作为 SaaS 产品需要精确追踪每次 LLM 调用的成本（token 数 × 单价）。每次调用产生一条 `SpendLog` 记录。在高并发场景下，如果每次调用都同步写 PostgreSQL，数据库写入压力会非常大，且会显著增加请求延迟。

## 决策

采用 **Redis Queue + 定时批量 Flush** 的异步写入模式。

```
请求完成 → CostTracker Hook (post_call)
             │
             ├── 计算 cost: tokens × price
             ├── 写入 Redis: RPUSH spend_queue {spend_entry_json}
             └── 返回客户端 (不等待 PG 写入)

定时任务 (60s):
  Redis: MULTI
           LRANGE spend_queue 0 999
           LTRIM spend_queue 1000 -1
         EXEC
         →  批量 INSERT INTO SpendLog
```

参考 LiteLLM 的 `DBSpendUpdateWriter` 设计。

## 理由

1. **请求延迟不增加**: 用户请求的延迟只包括 LLM 调用本身 + 写 Redis 的亚毫秒级延迟。PG 写入完全异步，对用户体验无影响。

2. **PG 写入压力可控**: 1 个 INSERT 操作写 1000 条记录 vs 1000 个 INSERT 操作各写 1 条记录，效率差一个数量级。

3. **Redis 天然适合队列**: RPUSH/LRANGE/LTRIM 操作都是 O(1)，且 Redis 数据允许丢失（极端情况下丢失几秒的 spend 数据可接受）。

4. **更新聚合字段**: `ApiKey.total_spend`、`Team.total_spend` 这类聚合字段同样通过 Redis 批量更新，不在请求路径上做 UPDATE。

5. **多实例友好**: 所有 Gateway 实例向同一个 Redis 队列写入，定时任务可以只在一个实例上运行（通过 Redis 分布式锁），避免重复 flush。

## 数据流

```
Gateway 实例 × N
  │  ┌──────────────────────────────────────┐
  ├──│ CostTracker.async_post_call_hook()    │
  │  │   ├── 计算 cost                       │
  │  │   └── RPUSH spend_queue {entry}       │
  │  └──────────────────────────────────────┘
  │
  ▼
Redis spend_queue: List<SpendEntry JSON>
  │
  │  (每 60s，分布式锁保证只有一个实例执行)
  ▼
SpendWriter.flush():
  ├── LLEN spend_queue (获取队列长度 N)
  ├── 如果 N == 0: 跳过
  ├── BATCH_SIZE = min(N, 1000)
  ├── MULTI
  │     LRANGE spend_queue 0 (BATCH_SIZE - 1)
  │     LTRIM spend_queue BATCH_SIZE -1
  ├── EXEC (原子执行，取出的条目不会因崩溃丢失)
  ├── 批量 INSERT INTO spend_logs (幂等：用 request_id 去重)
  └── UPDATE api_keys SET total_spend = total_spend + delta
```

## 后果

- **正面**: 请求延迟低，PG 写入压力小，多实例部署安全
- **负面**: 数据有短暂延迟（最多 60 秒），用户查用量可能看不到刚完成的请求
- **缓解**: 用量查询 API 同时查 PG + Redis 未 flush 的增量。读时合并，给用户近乎实时的数据
- **负面**: 极端情况下 Redis 故障会丢失未 flush 的 spend 数据
- **缓解**: 对大部分 SaaS 场景，丢失最多 60 秒的用量数据是可接受的。如需强一致性，可在 post_call hook 中同步写 PG 作为备份

## 备选方案

- **每请求同步写 PG**: 数据实时准确，但高并发下 PG 成为瓶颈，延迟显著增加
- **消息队列（Kafka/RabbitMQ）**: 可靠性更强，但对 MVP 过度设计，运维成本高
- **TimescaleDB / ClickHouse**: 时序数据写入性能更好，但引入新的数据库组件，运维复杂
