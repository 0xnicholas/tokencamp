use axum::{extract::State, Json};
use serde_json::{json, Value};
use tokencamp_core::ChatRequest;

use crate::{auth::KeyAuth, extractors::ValidJson, AppState};

pub async fn embeddings(
    State(state): State<AppState>,
    _auth: KeyAuth,
    ValidJson(request): ValidJson<ChatRequest>,
) -> Json<Value> {
    // v0.4 minimal: proxy to provider's embeddings endpoint
    // 通过 ChatRequest 复用 resolve_provider，调用 /v1/embeddings
    let model = request.model.clone();
    let (provider_config, api_key) = match state.resolve_provider(&model, &request).await {
        Ok(r) => r,
        Err(_) => return Json(json!({"error": "model not found"})),
    };

    let mut headers = reqwest::header::HeaderMap::new();
    let body = serde_json::to_value(&request).unwrap_or_default();

    let result = match state.handler.client()
        .post(format!("{}/v1/embeddings", provider_config.base_url()))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => match resp.json::<Value>().await {
            Ok(v) => v,
            Err(_) => json!({"error": "upstream error"}),
        },
        Err(_) => json!({"error": "upstream error"}),
    };

    Json(result)
}
