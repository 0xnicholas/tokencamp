use async_trait::async_trait;
use rand::seq::SliceRandom;

use crate::cache::CacheLayer;
use crate::provider::ProviderError;
use crate::types::{ChatRequest, ModelResponse};

use super::{DeploymentInfo, NoopTracking, RoutingStrategy};

pub struct SimpleShuffleStrategy;

#[async_trait]
impl RoutingStrategy for SimpleShuffleStrategy {
    fn name(&self) -> &'static str { "simple_shuffle" }

    async fn select_deployment<'a>(
        &self, deployments: &'a [DeploymentInfo], request: &ChatRequest, cache: &dyn CacheLayer,
    ) -> Option<&'a DeploymentInfo> {
        let idx = rand::random::<usize>() % deployments.len();
        deployments.get(idx)
    }

    async fn track_success(&self, d: &DeploymentInfo, r: &ModelResponse, dur: Option<u64>) {
        NoopTracking.track_success(d, r, dur).await;
    }

    async fn track_failure(&self, d: &DeploymentInfo, e: &ProviderError) {
        NoopTracking.track_failure(d, e).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_select_returns_one() {
        let strategy = SimpleShuffleStrategy;
        let deps = vec![
            DeploymentInfo { model_name: "m".into(), provider: "p1".into(), prompt_price: None, completion_price: None, tpm_limit: None, rpm_limit: None, tags: vec![] },
            DeploymentInfo { model_name: "m".into(), provider: "p2".into(), prompt_price: None, completion_price: None, tpm_limit: None, rpm_limit: None, tags: vec![] },
        ];
        let cache = crate::cache::DualCache::new_in_memory(10);
        let request = ChatRequest { model: "m".into(), messages: vec![], temperature: None, max_tokens: None, stream: None, extra: Default::default() };
        let selected = strategy.select_deployment(&deps, &request, &cache).await;
        assert!(selected.is_some());
    }
}
