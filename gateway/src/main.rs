mod config;
mod auth;
mod error;
mod routes;
mod extractors;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, routing::post};
use tokencamp_core::HttpHandler;
use tokencamp_core::ProviderConfig;
use tokencamp_core::provider::openai::OpenAiConfig;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<config::Config>,
    pub handler: Arc<HttpHandler>,
}

impl AppState {
    pub fn resolve_provider(
        &self,
        model_name: &str,
    ) -> Result<(Box<dyn ProviderConfig>, String), error::AppError> {
        let entry = self
            .config
            .model_list
            .iter()
            .find(|m| m.model_name == model_name)
            .ok_or(error::AppError::ModelNotFound {
                model: model_name.to_string(),
            })?;

        let provider = self
            .config
            .providers
            .get(&entry.provider)
            .ok_or(error::AppError::ProviderNotFound {
                provider: entry.provider.clone(),
            })?;

        let base_url = provider.base_url.clone();
        let api_key = provider.api_key.clone();

        let provider_config: Box<dyn ProviderConfig> = match entry.provider.as_str() {
            "openai" => Box::new(OpenAiConfig::new(base_url)),
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

    let state = AppState { config, handler };

    let app = Router::new()
        .route("/v1/chat/completions", post(routes::chat::chat_completions))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Tokencamp v0.1 listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
