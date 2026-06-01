use axum::{extract::State, Json};
use serde_json::{json, Value};
use tokencamp_core::ChatRequest;

use crate::{auth::KeyAuth, extractors::ValidJson, AppState};

/// Generic proxy helper
async fn proxy_to_provider(
    state: &AppState, request: &ChatRequest, path: &str,
) -> Json<Value> {
    let (provider_config, api_key) = match state.resolve_provider(&request.model, request).await {
        Ok(r) => r,
        Err(_) => return Json(json!({"error": "model not found"})),
    };
    let url = format!("{}{}", provider_config.base_url(), path);
    let result = state.handler.client()
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::to_value(request).unwrap_or_default())
        .send().await;
    match result {
        Ok(resp) => Json(resp.json().await.unwrap_or(json!({"error": "upstream error"}))),
        Err(_) => Json(json!({"error": "upstream error"})),
    }
}

pub async fn images_generations(State(state): State<AppState>, _auth: KeyAuth, ValidJson(request): ValidJson<ChatRequest>) -> Json<Value> {
    proxy_to_provider(&state, &request, "/v1/images/generations").await
}

pub async fn audio_transcriptions(State(state): State<AppState>, _auth: KeyAuth, ValidJson(request): ValidJson<ChatRequest>) -> Json<Value> {
    proxy_to_provider(&state, &request, "/v1/audio/transcriptions").await
}

pub async fn audio_speech(State(state): State<AppState>, _auth: KeyAuth, ValidJson(request): ValidJson<ChatRequest>) -> Json<Value> {
    proxy_to_provider(&state, &request, "/v1/audio/speech").await
}
