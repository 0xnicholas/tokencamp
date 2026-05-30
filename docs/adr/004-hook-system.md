# ADR-004: Hook 系统作为横切关注点框架

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp Gateway 有多个横切关注点需要在请求处理的特定阶段介入：并发限流、内容安全护栏、PII 检测、成本追踪、日志记录。这些关注点不应该污染核心路由逻辑，且需要在不同请求类型（chat、embedding、image）间复用。

Axum 的中间件基于 tower `Layer`，可以在请求/响应边界工作，但无法在「Router 调用前」和「响应回来后」这两个关键时点精确执行业务逻辑。

## 决策

采用 **Hook 注册表模式**，参考 LiteLLM 的 `proxy/hooks/` 设计：

```rust
// gateway/src/hooks/mod.rs
use std::collections::HashMap;
use once_cell::sync::Lazy;

static PROXY_HOOKS: Lazy<HashMap<&str, Box<dyn ProxyHook>>> = Lazy::new(|| {
    let mut m: HashMap<&str, Box<dyn ProxyHook>> = HashMap::new();
    m.insert("parallel_request_limiter", Box::new(ParallelRequestLimiter::new()));
    m.insert("content_guard", Box::new(ContentGuard::new()));
    m.insert("cost_tracker", Box::new(CostTracker::new()));
    m
});

pub fn get_hook(name: &str) -> Option<&dyn ProxyHook> {
    PROXY_HOOKS.get(name).map(AsRef::as_ref)
}
```

每个 Hook 实现三个生命周期方法：

```rust
#[async_trait]
pub trait ProxyHook: Send + Sync {
    async fn async_pre_call_hook(
        &self,
        request: &ChatRequest,
        auth: &AuthContext,
    ) -> Result<(), ErrorResponse>;

    async fn async_post_call_hook(
        &self,
        request: &ChatRequest,
        response: &ModelResponse,
        auth: &AuthContext,
    );

    async fn async_on_error_hook(
        &self,
        request: &ChatRequest,
        error: &GatewayError,
        auth: &AuthContext,
    );
}
```

Hook 通过 YAML 配置按需启用：

```yaml
hooks:
  enabled:
    - parallel_request_limiter
    - cost_tracker
```

## 理由

1. **关注点分离**: 核心路由逻辑（接收请求 → Router → 返回响应）保持简洁。所有横切逻辑通过 Hook 注入，互不干扰。

2. **按需启用**: 通过配置文件控制哪些 Hook 生效。开发环境可以只开 `cost_tracker`，生产环境开全部。Rust 的 trait 对象提供零成本的动态分发。

3. **独立测试**: 每个 Hook 是独立的 trait 实现，可以在 `#[cfg(test)]` 模块中单独构造输入进行单元测试，不需要启动完整的 Gateway。

4. **三个介入时点覆盖所有场景**:
   - `pre_call`: 判断是否允许调用（限流、护栏）→ 可以阻断
   - `post_call`: 调用成功后的异步处理（计费、日志）→ 不阻塞客户端
   - `on_error`: 调用失败后的处理（告警、错误聚合）

5. **比中间件更精细**: tower `Layer` 中间件只有 before-request 和 after-response 两个钩子。而 Hook 的三个时点分别对应业务逻辑的不同阶段，且可以直接访问解析好的 `request` 和 `auth_context`。

## 后果

- **正面**: 清晰的扩展点，新功能以 Hook 形式插入，不修改核心代码
- **负面**: Hook 的执行顺序有讲究（如必须先检查限流再检查内容），需要一个顺序保证机制
- **负面**: 如果某个 Hook 的执行耗时长（如同步调用外部服务做 PII 检测），会显著增加请求延迟（缓解：给 pre_call hooks 设置总超时）

## 备选方案

- **tower 中间件 (Layer)**: 简单，但只能访问原始 Request/Response，无法访问解析后的业务对象
- **Axum Extractors**: 可以注入到路由函数，但每个路由都要手动声明提取器，重复且容易遗漏
- **宏注解（Attribute Macros）**: 通过过程宏在每个路由函数上自动注入 Hook，灵活但增加编译复杂度和宏代码维护负担
