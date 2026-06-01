// gateway/src/router/mod.rs
pub mod cooldown;

use std::sync::Arc;

use tokencamp_core::{DeploymentInfo, RoutingStrategy};

use crate::config::ModelEntry;
use cooldown::CooldownManager;

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("no available deployment for model")]
    NoAvailableDeployment,
}

pub struct Router {
    cooldown: Arc<CooldownManager>,
    strategy: Box<dyn RoutingStrategy>,
}

impl Router {
    pub fn new(cooldown: CooldownManager, strategy: Box<dyn RoutingStrategy>) -> Self {
        Self { cooldown: Arc::new(cooldown), strategy }
    }

    pub fn cooldown(&self) -> &Arc<CooldownManager> {
        &self.cooldown
    }

    pub async fn select_deployment<'a>(
        &self, _model_name: &str, deployments: &'a [ModelEntry],
        request: &tokencamp_core::ChatRequest,
        fallbacks: &std::collections::HashMap<String, Vec<String>>,
        cache: &dyn tokencamp_core::CacheLayer,
    ) -> Result<&'a ModelEntry, RouterError> {
        // 1. 过滤 cooldown
        let available: Vec<&ModelEntry> = self.filter_cooldown(deployments).await;

        // 2. 策略选择
        let infos: Vec<DeploymentInfo> = available.iter().map(|m| model_entry_to_info(m)).collect();
        if let Some(selected) = self.strategy.select_deployment(&infos, request, cache).await {
            // 找到对应的原始 ModelEntry
            if let Some(entry) = available.iter().find(|m|
                m.model_name == selected.model_name && m.provider == selected.provider
            ) {
                return Ok(entry);
            }
        }

        // 3. Fallback
        if let Some(alt_models) = fallbacks.get(selected_model_name_from_first(&available)) {
            for alt in alt_models {
                // fallback 在当前列表或其他 deployments 中查找
                if let Some(dep) = deployments.iter().find(|d| d.model_name == *alt) {
                    let id = format!("{}:{}", dep.provider, dep.model_name);
                    if !self.cooldown.is_cooling_down(&id).await {
                        return Ok(dep);
                    }
                }
            }
        }

        Err(RouterError::NoAvailableDeployment)
    }

    async fn filter_cooldown<'a>(&self, deployments: &'a [ModelEntry]) -> Vec<&'a ModelEntry> {
        let mut available = Vec::new();
        for d in deployments {
            let id = format!("{}:{}", d.provider, d.model_name);
            if !self.cooldown.is_cooling_down(&id).await {
                available.push(d);
            }
        }
        available
    }

    pub async fn track_success(&self, dep: &ModelEntry, resp: &tokencamp_core::ModelResponse, duration_ms: Option<u64>) {
        let info = model_entry_to_info(dep);
        self.strategy.track_success(&info, resp, duration_ms).await;
    }

    pub async fn track_failure(&self, dep: &ModelEntry, err: &tokencamp_core::ProviderError) {
        let info = model_entry_to_info(dep);
        self.strategy.track_failure(&info, err).await;
    }
}

fn model_entry_to_info(m: &ModelEntry) -> DeploymentInfo {
    let info = m.litellm_params.as_ref().and_then(|p| p.model_info.as_ref());
    DeploymentInfo {
        model_name: m.model_name.clone(),
        provider: m.provider.clone(),
        prompt_price: info.map(|i| i.prompt_price),
        completion_price: info.map(|i| i.completion_price),
        tpm_limit: info.and_then(|i| i.tpm),
        rpm_limit: info.and_then(|i| i.rpm),
        tags: info.map(|i| i.tags.clone()).unwrap_or_default(),
    }
}

fn selected_model_name_from_first<'a>(available: &[&'a ModelEntry]) -> &'a str {
    available.first().map(|m| m.model_name.as_str()).unwrap_or("")
}
