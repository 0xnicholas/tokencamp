use axum::{
    extract::{FromRequest, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
use serde_json::json;

/// 包装 axum::Json，将反序列化错误转换为 OpenAI 风格 JSON 错误
pub struct ValidJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = JsonErrorResponse;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ValidJson(value)),
            Err(rejection) => {
                let msg = match &rejection {
                    JsonRejection::JsonDataError(e) => {
                        // 提取有意义的字段名和错误信息
                        let raw = e.to_string();
                        if raw.contains("missing field") {
                            // error 格式: "...missing field `model`..." 或 "...missing field 'model'..."
                            let field = raw
                                .split("missing field")
                                .nth(1)
                                .and_then(|s| {
                                    s.trim_start()
                                        .trim_start_matches(|c: char| c == '`' || c == '\'')
                                        .split(|c: char| c == '`' || c == '\'' || c == ' ')
                                        .next()
                                })
                                .unwrap_or("unknown");
                            format!("'{}' is required", field)
                        } else if raw.contains("invalid type") {
                            raw.replace(" at line", "")
                                .replace(" column", "")
                                .trim_start_matches("invalid type: ")
                                .to_string()
                        } else {
                            "invalid request body".to_string()
                        }
                    }
                    JsonRejection::JsonSyntaxError(_) => "invalid JSON".to_string(),
                    JsonRejection::MissingJsonContentType(_) => {
                        "Content-Type must be application/json".to_string()
                    }
                    _ => "invalid request".to_string(),
                };
                Err(JsonErrorResponse { message: msg })
            }
        }
    }
}

pub struct JsonErrorResponse {
    message: String,
}

impl IntoResponse for JsonErrorResponse {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "message": self.message,
                "type": "invalid_request_error"
            }
        });
        (StatusCode::BAD_REQUEST, Json(body)).into_response()
    }
}
