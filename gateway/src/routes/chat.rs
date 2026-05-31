use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use std::sync::Arc;
use tokencamp_core::{ChatRequest, ProviderError};

use crate::resilience::retry;
use crate::{auth::KeyAuth, error::AppError, extractors::ValidJson, AppState};

pub async fn chat_completions(
    State(state): State<AppState>,
    _auth: KeyAuth,
    ValidJson(request): ValidJson<ChatRequest>,
) -> Result<Response, AppError> {
    let stream = request.stream.unwrap_or(false);
    let model = request.model.clone();

    let (provider_config, api_key) = state.resolve_provider(&model).await?;
    let provider: Arc<dyn tokencamp_core::ProviderConfig> = provider_config.into();
    let deployment_id = format!("{}:{}", "resolved", &model);

    if stream {
        let handler = state.handler.clone();

        // Streaming: retry on initial connection failure only
        let wrapper = retry_with_cooldown(
            &state,
            &deployment_id,
            || {
                let provider = provider.clone();
                let request = request.clone();
                let api_key = api_key.clone();
                let handler = handler.clone();
                async move {
                    handler.complete_stream_owned(provider, request, api_key).await
                }
            },
        )
        .await?;

        let sse_stream = wrapper.map(|chunk| {
            let json = match chunk {
                Ok(c) => serde_json::to_string(&c).unwrap(),
                Err(e) => serde_json::json!({"error": {"message": e.to_string()}}).to_string(),
            };
            Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(json))
        });

        Ok(axum::response::Sse::new(sse_stream).into_response())
    } else {
        let response = retry_with_cooldown(
            &state,
            &deployment_id,
            || {
                let request = request.clone();
                let api_key = api_key.clone();
                let handler = state.handler.clone();
                let provider = provider.clone();
                async move {
                    handler.complete(&request, provider.as_ref(), &api_key).await
                }
            },
        )
        .await?;

        Ok(Json(response).into_response())
    }
}

/// Retry 循环 + cooldown 反馈
async fn retry_with_cooldown<T, F, Fut>(
    state: &AppState,
    deployment_id: &str,
    f: F,
) -> Result<T, AppError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, ProviderError>>,
{
    let max_retries = state.retry_config.num_retries;
    let base = state.retry_config.retry_after;
    let allowed_fails = state.config.router_settings.allowed_fails;

    let mut last_err: Option<ProviderError> = None;

    for attempt in 0..=max_retries {
        match f().await {
            Ok(val) => {
                state.cooldown.record_success(deployment_id).await;
                return Ok(val);
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

    // All retries exhausted or non-retryable error
    let failures = state.cooldown.record_failure(deployment_id).await;
    if failures >= allowed_fails {
        let _ = state
            .cooldown
            .mark(deployment_id, state.config.router_settings.cooldown_time)
            .await;
    }

    Err(AppError::Provider(last_err.unwrap_or(ProviderError::Timeout)))
}
