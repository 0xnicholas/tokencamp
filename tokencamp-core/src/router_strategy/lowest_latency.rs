use std::sync::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::cache::CacheLayer;
use crate::provider::ProviderError;
use crate::types::{ChatRequest, ModelResponse};

use super::{DeploymentInfo, RoutingStrategy};

pub struct LowestLatencyStrategy {
    cache: Arc<dyn CacheLayer>,
    window_size: usize,
    // in-memory fallback when Redis unavailable
    in_memory: Mutex<HashMap<String, Vec<f64>>>,
}

impl LowestLatencyStrategy {
    pub fn new(cache: Arc<dyn CacheLayer>, window_size: usize) -> Self {
        Self { cache, window_size, in_memory: Mutex::new(HashMap::new()) }
    }
}

#[async_trait]
impl RoutingStrategy for LowestLatencyStrategy {
    fn name(&self) -> &'static str { "lowest_latency" }

    async fn select_deployment<'a>(
        &self, deployments: &'a [DeploymentInfo], _request: &ChatRequest, _cache: &dyn CacheLayer,
    ) -> Option<&'a DeploymentInfo> {
        let mut best: Option<(&DeploymentInfo, f64)> = None;

        for dep in deployments {
            let key = format!("strategy:latency:{}:{}", dep.model_name, dep.provider);
            let avg = self.get_average_latency(&key).await;
            match best {
                None => best = Some((dep, avg)),
                Some((_, best_avg)) if avg < best_avg => best = Some((dep, avg)),
                _ => {}
            }
        }

        best.map(|(dep, _)| dep)
    }

    async fn track_success(
        &self, deployment: &DeploymentInfo, _response: &ModelResponse, duration_ms: Option<u64>,
    ) {
        let duration = duration_ms.unwrap_or(0) as f64;
        let key = format!("strategy:latency:{}:{}", deployment.model_name, deployment.provider);

        // Try Redis LPUSH + LTRIM
        // If fails or no Redis, use in-memory
        let mut map = self.in_memory.lock().unwrap();
        let list = map.entry(key).or_default();
        list.push(duration);
        if list.len() > self.window_size {
            list.remove(0);
        }
    }

    async fn track_failure(&self, deployment: &DeploymentInfo, _error: &ProviderError) {
        let key = format!("strategy:latency:{}:{}", deployment.model_name, deployment.provider);
        let mut map = self.in_memory.lock().unwrap();
        let list = map.entry(key).or_default();
        list.push(f64::MAX); // 惩罚
        if list.len() > self.window_size {
            list.remove(0);
        }
    }
}

impl LowestLatencyStrategy {
    async fn get_average_latency(&self, key: &str) -> f64 {
        let map = self.in_memory.lock().unwrap();
        if let Some(list) = map.get(key) {
            if list.is_empty() { return 0.0; }
            list.iter().sum::<f64>() / list.len() as f64
        } else {
            0.0 // 未调用过的 deployment，优先选择
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_select_fastest() {
        let cache = Arc::new(crate::cache::DualCache::new_in_memory(10));
        let strategy = LowestLatencyStrategy::new(cache.clone(), 10);

        let dep1 = DeploymentInfo { model_name: "m".into(), provider: "slow".into(), prompt_price: None, completion_price: None, tpm_limit: None, rpm_limit: None, tags: vec![] };
        let dep2 = DeploymentInfo { model_name: "m".into(), provider: "fast".into(), prompt_price: None, completion_price: None, tpm_limit: None, rpm_limit: None, tags: vec![] };

        // Track some latencies
        strategy.track_success(&dep1, &make_resp(), Some(1000)).await;
        strategy.track_success(&dep2, &make_resp(), Some(100)).await;

        let deps = vec![dep1, dep2];
        let request = ChatRequest { model: "m".into(), messages: vec![], temperature: None, max_tokens: None, stream: None, extra: Default::default() };
        let selected = strategy.select_deployment(&deps, &request, cache.as_ref()).await.unwrap();
        assert_eq!(selected.provider, "fast");
    }

    fn make_resp() -> ModelResponse {
        ModelResponse {
            id: "".into(), object: "".into(), created: 0, model: "m".into(),
            choices: vec![],
            usage: crate::types::Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
        }
    }
}
