// gateway/src/router/mod.rs
pub mod cooldown;

use std::sync::Arc;

use rand::seq::SliceRandom;

use crate::config::ModelEntry;

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("no available deployment for model")]
    NoAvailableDeployment,
}

pub struct Router {
    cooldown: Arc<cooldown::CooldownManager>,
}

impl Router {
    pub fn new(cooldown: cooldown::CooldownManager) -> Self {
        Self { cooldown: Arc::new(cooldown) }
    }

    pub fn cooldown(&self) -> &Arc<cooldown::CooldownManager> {
        &self.cooldown
    }

    pub async fn select_deployment<'a>(
        &self,
        model_name: &str,
        deployments: &'a [ModelEntry],
        fallbacks: &std::collections::HashMap<String, Vec<String>>,
    ) -> Result<&'a ModelEntry, RouterError> {
        // 1. 尝试主模型
        if let Some(dep) = self.try_select(deployments).await {
            return Ok(dep);
        }

        // 2. Fallback 链
        if let Some(alt_models) = fallbacks.get(model_name) {
            for alt in alt_models {
                // 注意：fallback 模型需要调用方传入对应的 deployments
                // 简化：fallback 只支持在当前列表中查找同 provider 的 deployment
                if let Some(dep) = deployments.iter().find(|d| d.model_name == *alt) {
                    if !self.cooldown.is_cooling_down(&format!("{}:{}", dep.provider, dep.model_name)).await {
                        return Ok(dep);
                    }
                }
            }
        }

        Err(RouterError::NoAvailableDeployment)
    }

    async fn try_select<'a>(&self, deployments: &'a [ModelEntry]) -> Option<&'a ModelEntry> {
        let mut available = Vec::new();
        for d in deployments {
            let id = format!("{}:{}", d.provider, d.model_name);
            if !self.cooldown.is_cooling_down(&id).await {
                available.push(d);
            }
        }
        if available.is_empty() { return None; }
        let mut rng = rand::thread_rng();
        Some(available.choose(&mut rng).unwrap())
    }
}
