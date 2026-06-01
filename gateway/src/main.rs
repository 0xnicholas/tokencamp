mod config;
mod auth;
mod error;
mod extractors;
mod routes;
mod router;
mod resilience;
mod db;
mod admin;
mod health;
mod metrics;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::{routing::{get, post}, Router};
use tokencamp_core::DualCache;
use tokencamp_core::HttpHandler;
use tokencamp_core::ProviderConfig;
use tokencamp_core::ProxyHook;
use tokencamp_core::RoutingStrategy;
use tokencamp_core::router_strategy::simple_shuffle::SimpleShuffleStrategy;
use tokencamp_core::router_strategy::lowest_cost::LowestCostStrategy;
use tokencamp_core::router_strategy::lowest_latency::LowestLatencyStrategy;
use tokencamp_core::router_strategy::usage_based::UsageBasedStrategy;
use tokencamp_core::router_strategy::tag_based::TagBasedStrategy;
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
    pub metrics: Arc<metrics::Metrics>,
}

impl AppState {
    pub async fn resolve_provider(
        &self,
        model_name: &str,
        request: &tokencamp_core::ChatRequest,
    ) -> Result<(Box<dyn ProviderConfig>, String), error::AppError> {
        let candidates: Vec<config::ModelEntry> = self
            .config
            .model_list
            .iter()
            .filter(|m| m.model_name == model_name)
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Err(error::AppError::ModelNotFound { model: model_name.to_string() });
        }

        let entry = if candidates.len() > 1 {
            self.app_router
                .select_deployment(model_name, &candidates, request, &self.config.router_settings.fallbacks, self.cache.as_ref())
                .await
                .map_err(|_| error::AppError::ModelNotFound { model: model_name.to_string() })?
                .clone()
        } else {
            candidates[0].clone()
        };

        let provider = self.config.providers.get(&entry.provider)
            .ok_or_else(|| error::AppError::ProviderNotFound { provider: entry.provider.clone() })?;
        let base_url = provider.base_url.clone();
        let api_key = provider.api_key.clone();
        let litellm_model = entry.litellm_params.as_ref().map(|p| p.model.clone()).unwrap_or_else(|| model_name.to_string());

        let provider_config: Box<dyn ProviderConfig> = match entry.provider.as_str() {
            "openai" => Box::new(OpenAiConfig::new(base_url)),
            "anthropic" => Box::new(AnthropicConfig::new(base_url, litellm_model, AnthropicMode::Chat)),
            other => return Err(error::AppError::ProviderNotFound { provider: other.to_string() }),
        };
        Ok((provider_config, api_key))
    }
}

fn build_hooks(cache: Arc<DualCache>) -> Vec<Box<dyn ProxyHook>> {
    let cache: Arc<dyn tokencamp_core::CacheLayer> = cache;
    vec![Box::new(ParallelRequestLimiter::new(cache.clone())), Box::new(CostTracker::new(cache))]
}

fn build_strategy(config: &config::Config, cache: Arc<dyn tokencamp_core::CacheLayer>) -> Box<dyn RoutingStrategy> {
    match config.router_settings.routing_strategy.as_str() {
        "lowest_cost" => Box::new(LowestCostStrategy),
        "lowest_latency" => Box::new(LowestLatencyStrategy::new(cache, config.router_settings.latency_window_size)),
        "usage_based" => Box::new(UsageBasedStrategy::new(cache)),
        "tag_based" => Box::new(TagBasedStrategy),
        _ => Box::new(SimpleShuffleStrategy),
    }
}

async fn metrics_handler(State(state): State<AppState>) -> String {
    state.metrics.render()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Arc::new(config::load("config/default.yaml").expect("Failed to load config"));
    let handler = Arc::new(HttpHandler::new());

    let db = match config.database_url.as_deref() {
        Some(url) if !url.is_empty() => Some(Arc::new(DbPool::new(url).await.expect("Failed to connect to PostgreSQL"))),
        _ => None,
    };

    let cache = match config.redis.url.as_deref() {
        Some(url) if !url.is_empty() => {
            let client = redis::Client::open(url).expect("Failed to create Redis client");
            let conn = client.get_multiplexed_async_connection().await.expect("Failed to connect to Redis");
            Arc::new(DualCache::new_redis(1000, conn))
        }
        _ => Arc::new(DualCache::new_in_memory(1000)),
    };

    let cooldown = CooldownManager::new_in_memory();
    let cache_ref: Arc<dyn tokencamp_core::CacheLayer> = cache.clone();
    let strategy = build_strategy(&config, cache_ref.clone());
    let app_router = Arc::new(router::Router::new(cooldown, strategy));
    let cooldown = app_router.cooldown().clone();
    let retry_config = Arc::new(RetryConfig::from(config.router_settings.clone()));
    let hooks = Arc::new(build_hooks(cache.clone()));
    let metrics = metrics::Metrics::new();

    let state = AppState { config, handler, app_router, cooldown, retry_config, cache, db, hooks, metrics };

    let hc = health::HealthChecker::new(state.config.clone());
    hc.start();

    let app = Router::new()
        .route("/v1/chat/completions", post(routes::chat::chat_completions))
        .route("/v1/messages", post(routes::messages::anthropic_messages))
        .route("/v1/models", get(routes::models::list_models))
        .route("/v1/embeddings", post(routes::embeddings::embeddings))
        .route("/metrics", get(metrics_handler))
        .route("/admin/keys/generate", post(admin::keys::generate_key))
        .route("/admin/keys", get(admin::keys::list_keys))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Tokencamp v0.4 listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
