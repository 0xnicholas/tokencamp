use std::sync::Arc;

use async_trait::async_trait;

use crate::cache::CacheLayer;
use crate::provider::ProviderError;
use crate::types::{ChatRequest, ModelResponse};

use super::{DeploymentInfo, RoutingStrategy};

pub struct UsageBasedStrategy {
    cache: Arc<dyn CacheLayer>,
}

impl UsageBasedStrategy {
    pub fn new(cache: Arc<dyn CacheLayer>) -> Self { Self { cache } }
}

#[async_trait]
impl RoutingStrategy for UsageBasedStrategy {
    fn name(&self) -> &'static str { "usage_based" }

    async fn select_deployment<'a>(
        &self, deployments: &'a [DeploymentInfo], _request: &ChatRequest, _cache: &dyn CacheLayer,
    ) -> Option<&'a DeploymentInfo> {
        let mut best: Option<(&DeploymentInfo, f64)> = None;

        for dep in deployments {
            let tpm_key = format!("strategy:usage:{}:{}:tpm", dep.model_name, dep.provider);
            let rpm_key = format!("strategy:usage:{}:{}:rpm", dep.model_name, dep.provider);

            let current_tpm = self.cache.get(&tpm_key).await
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            let current_rpm = self.cache.get(&rpm_key).await
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);

            let tpm_limit = dep.tpm_limit.unwrap_or(u32::MAX) as f64;
            let rpm_limit = dep.rpm_limit.unwrap_or(u32::MAX) as f64;

            let ratio = f64::max(
                if tpm_limit > 0.0 { current_tpm / tpm_limit } else { 0.0 },
                if rpm_limit > 0.0 { current_rpm / rpm_limit } else { 0.0 },
            );

            match best {
                None => best = Some((dep, ratio)),
                Some((_, best_r)) if ratio < best_r => best = Some((dep, ratio)),
                _ => {}
            }
        }

        best.map(|(dep, _)| dep)
    }

    async fn track_success(
        &self, deployment: &DeploymentInfo, response: &ModelResponse, _duration_ms: Option<u64>,
    ) {
        let tpm_key = format!("strategy:usage:{}:{}:tpm", deployment.model_name, deployment.provider);
        let rpm_key = format!("strategy:usage:{}:{}:rpm", deployment.model_name, deployment.provider);

        let _ = self.cache.incr(&tpm_key).await;
        let _ = self.cache.incr(&rpm_key).await;
        let _ = self.cache.expire(&tpm_key, 60).await;
        let _ = self.cache.expire(&rpm_key, 60).await;
    }

    async fn track_failure(&self, _d: &DeploymentInfo, _e: &ProviderError) {}
}
