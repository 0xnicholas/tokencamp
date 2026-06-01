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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_deployments() -> Vec<DeploymentInfo> {
        vec![
            DeploymentInfo { model_name: "m".into(), provider: "p1".into(), prompt_price: Some(10.0), completion_price: Some(5.0), tpm_limit: None, rpm_limit: None, tags: vec![] },
            DeploymentInfo { model_name: "m".into(), provider: "p2".into(), prompt_price: Some(5.0), completion_price: Some(3.0), tpm_limit: None, rpm_limit: None, tags: vec![] },
            DeploymentInfo { model_name: "m".into(), provider: "p3".into(), prompt_price: Some(20.0), completion_price: Some(10.0), tpm_limit: None, rpm_limit: None, tags: vec![] },
        ]
    }

    #[tokio::test]
    async fn test_select_cheapest() {
        let strategy = LowestCostStrategy;
        let deps = make_deployments();
        let cache = crate::cache::DualCache::new_in_memory(10);
        let request = ChatRequest { model: "m".into(), messages: vec![], temperature: None, max_tokens: None, stream: None, extra: Default::default() };
        let selected = strategy.select_deployment(&deps, &request, &cache).await.unwrap();
        assert_eq!(selected.provider, "p2"); // cheapest
    }
}
