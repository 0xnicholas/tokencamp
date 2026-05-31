// gateway/src/routes/messages.rs
use axum::{extract::State, Json};
use serde_json::Value;
use tokencamp_core::{ChatRequest, ProviderError};

use crate::{auth::KeyAuth, error::AppError, extractors::ValidJson, AppState};

pub async fn anthropic_messages(
    State(state): State<AppState>,
    _auth: KeyAuth,
    ValidJson(request): ValidJson<ChatRequest>,
) -> Result<Json<Value>, AppError> {
    let (provider_config, api_key) = state.resolve_provider(&request.model).await?;
    let raw = state
        .handler
        .complete_raw(&request, provider_config.as_ref(), &api_key)
        .await?;
    let value: Value = serde_json::from_slice(&raw)
        .map_err(|e| AppError::Provider(ProviderError::SerializationError(e)))?;
    Ok(Json(value))
}
