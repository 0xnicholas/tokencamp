use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cache::CacheLayer;
use crate::types::ChatRequest;

use super::{AuthContext, HookError, ProxyHook};

pub struct ParallelRequestLimiter {
    cache: Arc<dyn CacheLayer>,
}

impl ParallelRequestLimiter {
    pub fn new(cache: Arc<dyn CacheLayer>) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl ProxyHook for ParallelRequestLimiter {
    async fn async_pre_call_hook(
        &self, _request: &ChatRequest, auth: &AuthContext,
    ) -> Result<(), HookError> {
        let window = current_minute_window();
        let tpm_key = format!("key:{}:tpm:{}", auth.key_id, window);
        let rpm_key = format!("key:{}:rpm:{}", auth.key_id, window);

        // TPM check — always increment, then check
        let current_tpm = self.cache.incr(&tpm_key).await.map_err(|e| HookError::Internal(e))?;
        self.cache.expire(&tpm_key, 90).await;

        if let Some(limit) = auth.tpm_limit {
            if current_tpm > limit {
                return Err(HookError::RateLimitExceeded(format!("tpm limit {} exceeded (current: {})", limit, current_tpm)));
            }
        }

        // RPM check
        let current_rpm = self.cache.incr(&rpm_key).await.map_err(|e| HookError::Internal(e))?;
        self.cache.expire(&rpm_key, 90).await;

        if let Some(limit) = auth.rpm_limit {
            if current_rpm > limit {
                return Err(HookError::RateLimitExceeded(format!("rpm limit {} exceeded (current: {})", limit, current_rpm)));
            }
        }

        Ok(())
    }
}

fn current_minute_window() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 60
        * 60
}
