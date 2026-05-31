mod config;
mod auth;
mod error;
mod extractors;
mod routes;
mod router;
mod resilience;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::{get, post}, Router};
use tokencamp_core::HttpHandler;
use tokencamp_core::ProviderConfig;
use tokencamp_core::provider::anthropic::{AnthropicConfig, AnthropicMode};
use tokencamp_core::provider::openai::OpenAiConfig;

use crate::resilience::retry::RetryConfig;
use crate::router::cooldown::CooldownManager;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<config::Config>,
    pub handler: Arc<HttpHandler>,
    pub app_router: Arc<router::Router>,
    pub retry_config: Arc<RetryConfig>,
}

impl AppState {
    pub async fn resolve_provider(
        &self,
        model_name: &str,
    ) -> Result<(Box<dyn ProviderConfig>, String), error::AppError> {
        let candidates: Vec<config::ModelEntry> = self
            .config
            .model_list
            .iter()
            .filter(|m| m.model_name == model_name)
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Err(error::AppError::ModelNotFound {
                model: model_name.to_string(),
            });
        }

        let entry = if candidates.len() > 1 {
            self.app_router
                .select_deployment(model_name, &candidates)
                .await
                .map_err(|_| error::AppError::ModelNotFound {
                    model: model_name.to_string(),
                })?
                .clone()
        } else {
            candidates[0].clone()
        };

        let provider = self
            .config
            .providers
            .get(&entry.provider)
            .ok_or_else(|| error::AppError::ProviderNotFound {
                provider: entry.provider.clone(),
            })?;

        let base_url = provider.base_url.clone();
        let api_key = provider.api_key.clone();
        let litellm_model = entry
            .litellm_params
            .as_ref()
            .map(|p| p.model.clone())
            .unwrap_or_else(|| model_name.to_string());

        let provider_config: Box<dyn ProviderConfig> = match entry.provider.as_str() {
            "openai" => Box::new(OpenAiConfig::new(base_url)),
            "anthropic" => Box::new(AnthropicConfig::new(base_url, litellm_model, AnthropicMode::Chat)),
            other => {
                return Err(error::AppError::ProviderNotFound {
                    provider: other.to_string(),
                })
            }
        };

        Ok((provider_config, api_key))
    }
}

#[tokio::main]
async fn main() {
    let config = Arc::new(config::load("config/default.yaml").expect("Failed to load config"));
    let handler = Arc::new(HttpHandler::new());

    let cooldown = match &config.redis.url {
        Some(redis_url) => {
            let client = redis::Client::open(redis_url.as_str())
                .expect("Failed to create Redis client");
            let conn = client
                .get_multiplexed_async_connection()
                .await
                .expect("Failed to connect to Redis");
            CooldownManager::new_redis(conn)
        }
        None => CooldownManager::new_in_memory(),
    };

    let app_router = Arc::new(router::Router::new(cooldown));
    let retry_config = Arc::new(RetryConfig::from(config.router_settings.clone()));

    let state = AppState {
        config,
        handler,
        app_router,
        retry_config,
    };

    let app = Router::new()
        .route("/v1/chat/completions", post(routes::chat::chat_completions))
        .route("/v1/messages", post(routes::messages::anthropic_messages))
        .route("/v1/models", get(routes::models::list_models))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Tokencamp v0.2 listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
