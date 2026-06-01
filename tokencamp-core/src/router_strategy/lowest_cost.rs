use async_trait::async_trait;

use crate::cache::CacheLayer;
use crate::provider::ProviderError;
use crate::types::{ChatRequest, ModelResponse};

use super::{DeploymentInfo, RoutingStrategy};

pub struct LowestCostStrategy;

#[async_trait]
impl RoutingStrategy for LowestCostStrategy {
    fn name(&self) -> &'static str { "lowest_cost" }

    async fn select_deployment<'a>(
        &self, deployments: &'a [DeploymentInfo], _request: &ChatRequest, _cache: &dyn CacheLayer,
    ) -> Option<&'a DeploymentInfo> {
        deployments
            .iter()
            .min_by(|a, b| {
                let pa = a.prompt_price.unwrap_or(f64::MAX);
                let pb = b.prompt_price.unwrap_or(f64::MAX);
                pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    async fn track_success(&self, d: &DeploymentInfo, r: &ModelResponse, dur: Option<u64>) {
        super::NoopTracking.track_success(d, r, dur).await
    }
    async fn track_failure(&self, d: &DeploymentInfo, e: &ProviderError) {
        super::NoopTracking.track_failure(d, e).await
    }
}
