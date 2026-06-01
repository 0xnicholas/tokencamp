use async_trait::async_trait;

use crate::cache::CacheLayer;
use crate::provider::ProviderError;
use crate::types::{ChatRequest, ModelResponse};

use super::{DeploymentInfo, RoutingStrategy};

pub struct TagBasedStrategy;

#[async_trait]
impl RoutingStrategy for TagBasedStrategy {
    fn name(&self) -> &'static str { "tag_based" }

    async fn select_deployment<'a>(
        &self, deployments: &'a [DeploymentInfo], request: &ChatRequest, _cache: &dyn CacheLayer,
    ) -> Option<&'a DeploymentInfo> {
        let requested_tags: Vec<&str> = request
            .extra
            .get("_tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.as_str()).collect())
            .unwrap_or_default();

        if requested_tags.is_empty() {
            return deployments.first();
        }

        // 找到第一个匹配所有请求标签的 deployment
        deployments.iter().find(|d| {
            requested_tags.iter().all(|rt| d.tags.iter().any(|dt| dt == rt))
        })
    }

    async fn track_success(&self, d: &DeploymentInfo, r: &ModelResponse, dur: Option<u64>) {
        super::NoopTracking.track_success(d, r, dur).await
    }
    async fn track_failure(&self, d: &DeploymentInfo, e: &ProviderError) {
        super::NoopTracking.track_failure(d, e).await
    }
}
