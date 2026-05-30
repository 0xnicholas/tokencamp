# ADR-013: 密钥管理

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp 需要管理两类密钥：(1) Gateway 自己的密钥（master_key、数据库连接、Redis 密码）；(2) Provider 凭证（OpenAI API Key、Anthropic API Key），这些是上游 LLM 提供商的密钥，不能以明文形式存储在配置或数据库中。

## 决策

采用 **分层密钥管理**，区分「配置密钥」和「Provider 凭证」。

### 配置密钥

Gateway 自身的敏感配置（master_key、数据库密码）通过环境变量注入，不在任何文件中明文存储：

```yaml
# config/default.yaml
general_settings:
  master_key: ${TOKENCAMP_MASTER_KEY}   # 运行时从环境变量解析
```

Docker / K8s 中通过 Secret 注入环境变量。

### Provider 凭证

Provider API Key 有三种存储方式，按安全等级递进：

| 方式 | 存储位置 | 加密 | 适用场景 |
|------|---------|------|---------|
| **环境变量** | Gateway 进程环境 | 否 | 开发环境、全局共享凭证 |
| **加密数据库** | Credential 表 `credential_values` 列 | AES-256-GCM（`aes-gcm` crate），密钥来自环境变量 `ENCRYPTION_KEY` | SaaS 生产环境，每租户可配独立凭证 |
| **外部 Secret Manager** | AWS Secrets Manager / GCP Secret Manager / Vault | 传输 + 静态加密 | 企业级部署，密钥轮换 |

### 加密存储流程（方式 2：加密数据库）

```
写入:
  admin 通过管理 API 提交 api_key 明文
  → 服务端: AES-256-GCM(api_key, ENCRYPTION_KEY)
  → ciphertext 存入 credential_values (JSON 列)

读取:
  Router 需要调用 Provider
  → 检查内存缓存（LRU，TTL 5min）：命中直接返回
  → 缓存 miss：从 DB 读取 ciphertext
  → AES-256-GCM-Decrypt(ciphertext, ENCRYPTION_KEY)
  → 明文注入到 litellm_params 的 HTTP header
  → 明文仅在进程内存中存在，不落盘、不记录日志
  → 写入缓存
```

### 外部 Secret Manager（方式 3）

```rust
// secret_managers/mod.rs
#[async_trait]
pub trait SecretManager: Send + Sync {
    async fn get_secret(&self, secret_name: &str) -> Result<String, SecretError>;
}

// 使用示例
let api_key = secret_manager.get_secret("tokencamp/openai-prod-key").await?;
```

Provider 配置中用 `secret_name` 替代 `api_key`：

```yaml
model_list:
  - model_name: gpt-5
    litellm_params:
      model: openai/gpt-5
      secret_name: tokencamp/openai-prod-key  # 从 Secret Manager 获取
```

Gateway 启动时检测 `secret_name` 字段，通过 Secret Manager 解析为实际 API Key。

### 密钥轮换

- **Provider 凭证轮换**: 管理 API `POST /manage/credentials/{id}/rotate` → 更新 DB encrypted value → Redis Pub/Sub 通知所有实例刷新
- **加密密钥轮换**: `ENCRYPTION_KEY` 变更后，需要重新加密数据库中的所有现有凭证。提供一个 CLI 子命令：`tokencamp rotate-encryption-key --old-key X --new-key Y`

## 后果

- **正面**: 密钥不在代码、配置文件或日志中。支持多种安全等级
- **负面**: 加密数据库方式依赖 `ENCRYPTION_KEY` 环境变量，密钥泄漏会导致所有凭证泄漏
- **缓解**: `ENCRYPTION_KEY` 从 Secret Manager 或其他安全机制注入。生产环境推荐使用外部 Secret Manager

## 备选方案

- **仅环境变量**: 最简单，但无法支持多租户各自配置独立 Provider 凭证
- **Hashicorp Vault 独占**: 安全等级最高，但 Vault 自身运维复杂，MVP 过度
