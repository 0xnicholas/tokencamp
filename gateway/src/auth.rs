use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use crate::AppState;

pub struct KeyAuth;

impl FromRequestParts<AppState> for KeyAuth {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing authorization header"))?;

        let key = auth_header
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "invalid authorization format"))?;

        if state.config.auth.api_keys.iter().any(|k| k == key) {
            Ok(KeyAuth)
        } else {
            Err((StatusCode::UNAUTHORIZED, "invalid api key"))
        }
    }
}
