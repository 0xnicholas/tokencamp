use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use std::sync::Arc;
use tokencamp_core::{ChatRequest, ModelResponse};

use crate::{auth::KeyAuth, error::AppError, extractors::ValidJson, AppState};

pub async fn chat_completions(
    State(state): State<AppState>,
    _auth: KeyAuth,
    ValidJson(request): ValidJson<ChatRequest>,
) -> Result<Response, AppError> {
    let stream = request.stream.unwrap_or(false);
    let model = request.model.clone();

    let (provider_config, api_key) = state.resolve_provider(&model).await?;

    if stream {
        let provider: Arc<dyn tokencamp_core::ProviderConfig> = provider_config.into();

        let wrapper = state
            .handler
            .clone()
            .complete_stream_owned(provider, request, api_key)
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
        let response = state
            .handler
            .complete(&request, provider_config.as_ref(), &api_key)
            .await?;
        Ok(Json(response).into_response())
    }
}
