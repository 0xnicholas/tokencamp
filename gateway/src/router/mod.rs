// gateway/src/router/mod.rs
pub mod cooldown;

use rand::seq::SliceRandom;

use crate::config::ModelEntry;
use cooldown::CooldownManager;

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("no available deployment for model")]
    NoAvailableDeployment,
}

pub struct Router {
    cooldown: CooldownManager,
}

impl Router {
    pub fn new(cooldown: CooldownManager) -> Self {
        Self { cooldown }
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

    pub fn cooldown_manager(&self) -> &CooldownManager {
        &self.cooldown
    }
}
