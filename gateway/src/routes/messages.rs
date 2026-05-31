use axum::{extract::State, Json};
use serde_json::Value;
use tokencamp_core::{ChatRequest, ProviderError};

use crate::resilience::retry;
use crate::{auth::KeyAuth, error::AppError, extractors::ValidJson, AppState};

pub async fn anthropic_messages(
    State(state): State<AppState>,
    _auth: KeyAuth,
    ValidJson(request): ValidJson<ChatRequest>,
) -> Result<Json<Value>, AppError> {
    let model = request.model.clone();
    let (provider_config, api_key) = state.resolve_provider(&model).await?;
    let deployment_id = format!("{}:{}", "resolved", &model);

    let max_retries = state.retry_config.num_retries;
    let base = state.retry_config.retry_after;
    let allowed_fails = state.config.router_settings.allowed_fails;

    let mut last_err: Option<ProviderError> = None;

    for attempt in 0..=max_retries {
        match state
            .handler
            .complete_raw(&request, provider_config.as_ref(), &api_key)
            .await
        {
            Ok(raw) => {
                state.cooldown.record_success(&deployment_id).await;
                let value: Value = serde_json::from_slice(&raw)
                    .map_err(|e| AppError::Provider(ProviderError::SerializationError(e)))?;
                return Ok(Json(value));
            }
            Err(e) if retry::is_retryable(&e) && attempt < max_retries => {
                last_err = Some(e);
                let delay = retry::retry_delay(attempt, base);
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                last_err = Some(e);
                break;
            }
        }
    }

    let failures = state.cooldown.record_failure(&deployment_id).await;
    if failures >= allowed_fails {
        let _ = state
            .cooldown
            .mark(&deployment_id, state.config.router_settings.cooldown_time)
            .await;
    }

    Err(AppError::Provider(last_err.unwrap_or(ProviderError::Timeout)))
}
