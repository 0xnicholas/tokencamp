# ADR-016: 可观测性

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp 作为 SaaS 产品需要全面的可观测性：(1) 运维团队需要监控 Gateway 和 Provider 的健康状态；(2) 需要追踪每个请求的完整链路以排查问题；(3) 需要计量指标用于计费和容量规划。三层需求对应可观测性三大支柱：Metrics、Logging、Tracing。

## 决策

### Metrics（指标）

使用 **Prometheus** 作为指标收集和查询标准，Gateway 暴露 `/metrics` 端点。

**核心指标：**

| 指标 | 类型 | 标签 | 用途 |
|------|------|------|------|
| `tokencamp_requests_total` | Counter | `model, provider, status` | 请求量统计 |
| `tokencamp_request_duration_seconds` | Histogram | `model, provider` | 延迟分布 (P50/P95/P99) |
| `tokencamp_tokens_total` | Counter | `model, provider, type(prompt\|completion)` | Token 用量 |
| `tokencamp_cost_dollars_total` | Counter | `model, provider` | 费用统计 |
| `tokencamp_errors_total` | Counter | `model, provider, error_type` | 错误分类 |
| `tokencamp_deployment_health` | Gauge | `deployment_id, status(0\|1)` | Deployment 健康 |
| `tokencamp_rate_limited_total` | Counter | `reason(tpm\|rpm)` | 限流触发次数 |
| `tokencamp_active_connections` | Gauge | — | 活跃 SSE 连接数 |
| `tokencamp_db_latency_seconds` | Histogram | `operation` | DB 查询延迟 |

实现方式：在每个 Hook 的关键点 emit Counter/Histogram，通过 Rust `metrics` + `metrics-exporter-prometheus` crate 暴露 `/metrics` 端点。也可使用 `prometheus` crate 直接操作指标注册表。

### Logging（日志）

使用 **结构化日志**（JSON 格式），每行一条完整事件。Rust 端通过 `tracing` + `tracing-subscriber` 的 JSON layer 输出：

```json
{
  "timestamp": "2026-05-29T10:30:00.123Z",
  "level": "INFO",
  "event": "request_completed",
  "request_id": "req_abc123",
  "key_id": "key_456",
  "model": "gpt-5",
  "provider": "openai",
  "stream": false,
  "tokens": {"prompt": 150, "completion": 400},
  "cost": 0.00475,
  "duration_ms": 1234,
  "status": "success"
}
```

**日志级别约定：**
- `INFO`: 正常请求完成
- `WARN`: 重试/fallback 触发、限流阈值告警
- `ERROR`: Provider 调用失败、DB/Redis 连接失败

敏感字段（API Key 明文、Token 内容）从不写入日志。通过 `SensitiveDataMasker` 层在 tracing span 创建前清理。

Gateway 输出 JSON 到 stdout，由 Docker/K8s 日志驱动收集。不内置日志聚合（由外部 ELK/Loki 负责）。

### Tracing（链路追踪）

使用 **OpenTelemetry** 标准，支持导出到 Jaeger / Datadog / OTLP collector。

**Span 层级：**

```
Gateway Request (root span, request_id)
  ├── Auth Check (span)
  │     ├── Redis: GET key_cache
  │     └── PG: SELECT api_key (if cache miss)
  ├── Pre-call Hooks (span)
  │     ├── parallel_request_limiter
  │     └── content_guard
  ├── Router.select_deployment (span)
  │     ├── Redis: GET cooldown
  │     └── Strategy: lowest_latency
  ├── Provider Call (span, model + provider)
  │     ├── HTTP POST to Provider
  │     └── Stream chunks (span per chunk batch)
  └── Post-call Hooks (span)
        └── cost_tracker: RPUSH spend_queue
```

实现方式：
- `axum-tracing-opentelemetry` 或 `tower-http` trace layer 中间件自动创建 root span
- 关键操作内部通过 `tracing` crate 的 `#[instrument]` 宏手动创建子 span
- Span 属性通过 `tracing-opentelemetry` 的 `OpenTelemetrySpanExt` 注入 `key_id`, `model`, `deployment_id` 等业务标识

### 告警规则

```yaml
# prometheus.rules.yml
groups:
  - name: tokencamp
    rules:
      - alert: HighErrorRate
        expr: rate(tokencamp_errors_total[5m]) / rate(tokencamp_requests_total[5m]) > 0.05
        annotations:
          summary: "错误率超过 5%"

      - alert: DeploymentUnhealthy
        expr: tokencamp_deployment_health == 0
        for: 2m
        annotations:
          summary: "Deployment {{ $labels.deployment_id }} 不健康超过 2 分钟"

      - alert: HighLatency
        expr: histogram_quantile(0.95, tokencamp_request_duration_seconds) > 10
        for: 5m
        annotations:
          summary: "P95 延迟超过 10 秒"
```

## 后果

- **正面**: Prometheus + OTel 是行业标准，与 K8s 生态无缝集成。指标可直接用于 Grafana dashboard 和自动扩缩容
- **负面**: OpenTelemetry 引入额外依赖和性能开销（span 创建、属性序列化）
- **缓解**: 通过采样控制 tracing 开销。MVP 默认采样率 10%，生产环境按需调整。Metrics 开销极小（Counter/Histogram 是内存操作）

## 备选方案

- **Datadog 全家桶**: 开箱即用但厂商锁定，成本随规模增长
- **ELK 三件套**: 日志聚合强大，但指标和时间序列支持不如 Prometheus
- **裸 stdout + grep**: 最简单的「可观测性」，但不适合多实例部署
