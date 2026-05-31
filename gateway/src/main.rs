mod config;
mod auth;
mod error;
mod routes;

use std::sync::Arc;
use tokencamp_core::HttpHandler;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<config::Config>,
    pub handler: Arc<HttpHandler>,
}

fn main() {
    let config = Arc::new(config::load("config/default.yaml").expect("Failed to load config"));
    let _state = AppState {
        config,
        handler: Arc::new(HttpHandler::new()),
    };
    println!("Auth module compiled successfully");
}
