mod config;
mod auth;
mod error;
mod extractors;
mod routes;
mod router;
mod resilience;
mod db;
mod admin;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::{delete, get, post}, Router};
use tokencamp_core::DualCache;
use tokencamp_core::HttpHandler;
use tokencamp_core::ProviderConfig;
use tokencamp_core::ProxyHook;
use tokencamp_core::provider::anthropic::{AnthropicConfig, AnthropicMode};
use tokencamp_core::provider::openai::OpenAiConfig;
use tokencamp_core::hooks::parallel_request_limiter::ParallelRequestLimiter;
use tokencamp_core::hooks::cost_tracker::CostTracker;

use crate::resilience::retry::RetryConfig;
use crate::router::cooldown::CooldownManager;
use crate::db::DbPool;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<config::Config>,
    pub handler: Arc<HttpHandler>,
    pub app_router: Arc<router::Router>,
    pub cooldown: Arc<CooldownManager>,
    pub retry_config: Arc<RetryConfig>,
    pub cache: Arc<DualCache>,
    pub db: Option<Arc<DbPool>>,
    pub hooks: Arc<Vec<Box<dyn ProxyHook>>>,
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

fn build_hooks(cache: Arc<DualCache>) -> Vec<Box<dyn ProxyHook>> {
    let cache: Arc<dyn tokencamp_core::CacheLayer> = cache;
    vec![
        Box::new(ParallelRequestLimiter::new(cache.clone())),
        Box::new(CostTracker::new(cache)),
    ]
}

#[tokio::main]
async fn main() {
    let config = Arc::new(config::load("config/default.yaml").expect("Failed to load config"));
    let handler = Arc::new(HttpHandler::new());

    // PostgreSQL
    let db = if let Some(ref url) = config.database_url {
        if !url.is_empty() {
            Some(Arc::new(DbPool::new(url).await.expect("Failed to connect to PostgreSQL")))
        } else {
            None
        }
    } else {
        None
    };

    // Redis / DualCache
    let cache = match config.redis.url.as_deref() {
        Some(url) if !url.is_empty() => {
            let client = redis::Client::open(url).expect("Failed to create Redis client");
            let conn = client.get_multiplexed_async_connection().await
                .expect("Failed to connect to Redis");
            Arc::new(DualCache::new_redis(1000, conn))
        }
        _ => Arc::new(DualCache::new_in_memory(1000)),
    };

    // Cooldown (shared with Router)
    let cooldown = CooldownManager::new_in_memory(); // v0.3: reuse DualCache for cooldowns later
    let app_router = Arc::new(router::Router::new(cooldown));
    let cooldown = app_router.cooldown().clone();
    let retry_config = Arc::new(RetryConfig::from(config.router_settings.clone()));

    // Hooks
    let hooks = Arc::new(build_hooks(cache.clone()));

    let state = AppState {
        config,
        handler,
        app_router,
        cooldown,
        retry_config,
        cache,
        db,
        hooks,
    };

    let app = Router::new()
        .route("/v1/chat/completions", post(routes::chat::chat_completions))
        .route("/v1/messages", post(routes::messages::anthropic_messages))
        .route("/v1/models", get(routes::models::list_models))
        .route("/admin/keys/generate", post(admin::keys::generate_key))
        .route("/admin/keys", get(admin::keys::list_keys))
        // .route("/admin/keys/{id}", delete(admin::keys::delete_key))  // TODO: fix trait bound
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Tokencamp v0.3 listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
