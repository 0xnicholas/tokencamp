use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;

pub struct HealthChecker {
    config: Arc<Config>,
}

impl HealthChecker {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    pub fn start(self) {
        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();

            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;

                for entry in &self.config.model_list {
                    let provider = match self.config.providers.get(&entry.provider) {
                        Some(p) => p,
                        None => continue,
                    };

                    let base = provider.base_url.as_deref().unwrap_or("https://api.openai.com");
                    let url = format!("{}/v1/models", base);

                    match client.get(&url).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            // healthy
                        }
                        _ => {
                            eprintln!("[health] {} deployment unhealthy: {}", entry.model_name, url);
                        }
                    }
                }
            }
        });
    }
}
