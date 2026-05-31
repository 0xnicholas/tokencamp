// gateway/src/resilience/retry.rs
use std::time::Duration;
use tokencamp_core::ProviderError;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub num_retries: u32,
    pub retry_after: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            num_retries: 3,
            retry_after: 0.5,
        }
    }
}

impl From<crate::config::RouterSettings> for RetryConfig {
    fn from(s: crate::config::RouterSettings) -> Self {
        Self {
            num_retries: s.num_retries,
            retry_after: s.retry_after,
        }
    }
}

pub fn is_retryable(e: &ProviderError) -> bool {
    matches!(
        e,
        ProviderError::UpstreamError {
            status: 429, ..
        } | ProviderError::UpstreamError {
            status: 500..=599, ..
        } | ProviderError::Timeout
    )
}

pub fn retry_delay(attempt: u32, base: f64) -> Duration {
    let ms = (base * 1000.0 * 2f64.powi(attempt as i32)) as u64 + rand::random::<u64>() % 100;
    Duration::from_millis(ms)
}
