use axum::{extract::State, Json};
use tokencamp_core::{ChatRequest, ModelResponse};

use crate::{error::AppError, auth::KeyAuth, AppState};

pub async fn chat_completions(
    State(state): State<AppState>,
    _auth: KeyAuth,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ModelResponse>, AppError> {
    if request.stream.unwrap_or(false) {
        return Err(AppError::StreamNotSupported);
    }

    let (provider_config, api_key) = state.resolve_provider(&request.model)?;
    let response = state
        .handler
        .complete(&request, provider_config.as_ref(), &api_key)
        .await?;
    Ok(Json(response))
}
