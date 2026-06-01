use async_trait::async_trait;

use crate::cache::CacheLayer;
use crate::provider::ProviderError;
use crate::types::{ChatRequest, ModelResponse};

/// Gateway 层使用的 Deployment 条目（简化自 config::ModelEntry）
/// Core 不依赖 gateway config，使用此 trait 关联类型
pub struct DeploymentInfo {
    pub model_name: String,
    pub provider: String,
    pub prompt_price: Option<f64>,
    pub completion_price: Option<f64>,
    pub tpm_limit: Option<u32>,
    pub rpm_limit: Option<u32>,
    pub tags: Vec<String>,
}

#[async_trait]
pub trait RoutingStrategy: Send + Sync {
    fn name(&self) -> &'static str;

    async fn select_deployment<'a>(
        &self, deployments: &'a [DeploymentInfo], request: &ChatRequest, cache: &dyn CacheLayer,
    ) -> Option<&'a DeploymentInfo>;

    async fn track_success(
        &self, deployment: &DeploymentInfo, response: &ModelResponse, duration_ms: Option<u64>,
    );

    async fn track_failure(
        &self, deployment: &DeploymentInfo, error: &ProviderError,
    );
}

pub mod simple_shuffle;
pub mod lowest_cost;
pub mod lowest_latency;
pub mod usage_based;
pub mod tag_based;

/// 空操作 tracker（供无状态策略复用）
pub struct NoopTracking;

impl NoopTracking {
    pub async fn track_success(&self, _d: &DeploymentInfo, _r: &ModelResponse, _dur: Option<u64>) {}
    pub async fn track_failure(&self, _d: &DeploymentInfo, _e: &ProviderError) {}
}
