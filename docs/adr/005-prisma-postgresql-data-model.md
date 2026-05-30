# ADR-005: SQLx + PostgreSQL 作为数据层

**状态**: 已采纳

**日期**: 2026-05-30

**决策者**: Nicholas

## 背景

Tokencamp 的 API Gateway 需要管理 API Key 认证、用量追踪（SpendLog）、部署配置（Deployment）、Provider 凭证（Credential）等实体。需要一个类型安全、异步原生、迁移管理方便的数据访问方案。

## 决策

使用 **SQLx + PostgreSQL 16** 作为数据层。SQLx 是 Rust 生态中最成熟的异步数据库驱动，提供编译期 SQL 验证和类型安全的查询结果。

## 理由

1. **编译期 SQL 验证**: SQLx 的 `sqlx::query!()` 和 `sqlx::query_as!()` 宏在编译时连接数据库验证 SQL 语法和列类型，将 SQL 错误从运行时提前到编译期。这是 Rust 独有的安全优势，比任何 ORM 的运行时验证都更可靠。

2. **类型安全的查询结果**: 查询结果自动反序列化为 Rust 结构体（通过 `serde`），类型完全推导。`sqlx::query_as!(ApiKey, "SELECT * FROM api_keys WHERE key_hash = $1", hash)` 返回 `Result<ApiKey>`，编译期保证字段名和类型匹配。

3. **原生异步**: SQLx 基于 tokio 构建，所有数据库操作都是 `async`，零阻塞。不需要像 Prisma Client Python 那样依赖单独的异步适配层。在高并发 Gateway 场景下，连接池复用和异步 IO 开箱即用。

4. **SQL 迁移管理**: `sqlx migrate` 提供纯 SQL 迁移文件管理（`migrations/20260101_initial_schema.up.sql`），比 Prisma 的声明式 schema 更灵活。复杂查询（窗口函数、CTE、聚合）直接写 SQL，不需要通过 ORM query builder。

5. **无 ORM 开销**: SQLx 不是 ORM，不生成额外的查询抽象层。每个查询就是手写的精确 SQL，性能可预期，没有 N+1 查询隐患。对于 SpendLog 批量写入和聚合查询等性能敏感场景，可以精细控制 SQL。

6. **PostgreSQL 的成熟生态**: JSONB 列（`model_info`、`provider_params`）、部分索引（`WHERE` 子句）、BRIN 索引（时序数据）等特性完美契合配置管理和时序追踪场景。SQLx 对这些类型有原生支持。

## 核心实体

```
ApiKey ──── SpendLog

Deployment (独立，不属于 Key)
Credential (独立，Provider 凭证加密存储)
```

每条 SpendLog 记录 `key_id`、`model`、`provider`、`tokens`、`cost`、`request_id`。聚合字段 `total_spend` 在 ApiKey 上维护。

## Schema 管理方式

与 Prisma 的声明式 schema DSL 不同，SQLx 使用纯 SQL 迁移文件：

```sql
-- migrations/20260101_initial_schema.up.sql
CREATE TABLE api_keys (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash    TEXT NOT NULL UNIQUE,
    name        TEXT,
    tpm_limit   INTEGER,
    rpm_limit   INTEGER,
    total_spend DOUBLE PRECISION NOT NULL DEFAULT 0,
    is_active   BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE spend_logs (
    id          BIGSERIAL PRIMARY KEY,
    key_id      UUID REFERENCES api_keys(id),
    request_id  TEXT NOT NULL UNIQUE,
    model       TEXT NOT NULL,
    provider    TEXT NOT NULL,
    prompt_tokens   INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    cost        DOUBLE PRECISION NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    status      TEXT NOT NULL DEFAULT 'success',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_spend_logs_key ON spend_logs(key_id);
CREATE INDEX idx_spend_logs_created ON spend_logs USING BRIN (created_at);
CREATE INDEX idx_spend_logs_request ON spend_logs(request_id);
```

Rust 中使用编译期验证的查询：

```rust
// 类型安全的查询（编译时验证 SQL 和列类型）
let key = sqlx::query_as!(
    ApiKey,
    r#"SELECT id, key_hash, name, tpm_limit, rpm_limit,
              total_spend, is_active, created_at
       FROM api_keys WHERE key_hash = $1"#,
    hash
)
.fetch_optional(&pool)
.await?;

// 聚合查询也享受编译期检查
let stats = sqlx::query_as!(
    KeySpendStats,
    r#"SELECT key_id, SUM(cost) as total_cost, COUNT(*) as request_count
       FROM spend_logs
       WHERE created_at >= $1
       GROUP BY key_id"#,
    since
)
.fetch_all(&pool)
.await?;
```

## 后果

- **正面**: 编译期 SQL 验证，类型安全的查询，原生异步，无 ORM 开销，精确的 SQL 控制
- **负面**: 没有声明式 schema DSL，表关系定义在 SQL 中，跨表关系需要在应用层组织（不如 Prisma 的 `include` 嵌套查询直观）
- **负面**: SQLx 宏依赖编译时数据库连接（`DATABASE_URL` 环境变量），CI 环境需要运行 PostgreSQL 服务
- **缓解**: 使用 `sqlx::query_as`（非宏版本，无编译期验证）作为 fallback；高频写入路径（SpendLog）使用 Redis queue + 批量 flush，不通过 SQLx 逐条写入

## 备选方案

- **Diesel ORM**: Rust 最成熟的 ORM，有 schema DSL 和强类型查询 builder。但同步为主，异步支持不完善，不适合 tokio/axum 生态。
- **SeaORM**: 基于 SQLx 的异步 ORM，有 ActiveRecord 风格 API 和关系加载。比 SQLx 抽象层次高，但引入额外依赖和查询生成的不确定性。
- **tokio-postgres（裸驱动）**: 最灵活、零开销，但没有编译期 SQL 验证和类型安全的查询结果映射，需要大量手动 serde 代码。
