use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use tokencamp_core::ProviderError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("model not found: {model}")]
    ModelNotFound { model: String },

    #[error("provider not found: {provider}")]
    ProviderNotFound { provider: String },

    #[error("streaming not supported in v0.1")]
    StreamNotSupported,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Provider(ProviderError::UpstreamError { status: s, message: m }) => {
                (StatusCode::from_u16(*s).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), m.clone())
            }
            AppError::Provider(ProviderError::Timeout) => {
                (StatusCode::GATEWAY_TIMEOUT, "upstream timeout".to_string())
            }
            AppError::Provider(_) => {
                (StatusCode::BAD_GATEWAY, "upstream error".to_string())
            }
            AppError::ModelNotFound { model } => {
                (StatusCode::BAD_REQUEST, format!("model '{}' not found", model))
            }
            AppError::ProviderNotFound { provider } => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("provider '{}' not configured", provider))
            }
            AppError::StreamNotSupported => {
                (StatusCode::BAD_REQUEST, "streaming is not supported in this version".to_string())
            }
        };

        let body = serde_json::json!({ "error": { "message": message } });
        (status, Json(body)).into_response()
    }
}
