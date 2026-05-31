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
        _model_name: &str,
        deployments: &'a [ModelEntry],
    ) -> Result<&'a ModelEntry, RouterError> {
        // 过滤 cooldown 中的 Deployment
        let mut available = Vec::new();
        for d in deployments {
            let id = format!("{}:{}", d.provider, d.model_name);
            if !self.cooldown.is_cooling_down(&id).await {
                available.push(d);
            }
        }

        if available.is_empty() {
            return Err(RouterError::NoAvailableDeployment);
        }

        // simple-shuffle
        let mut rng = rand::thread_rng();
        Ok(available.choose(&mut rng).unwrap())
    }
}
