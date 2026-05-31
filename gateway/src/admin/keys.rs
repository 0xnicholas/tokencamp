use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Sha256, Digest};
use uuid::Uuid;

use crate::AppState;

pub struct MasterKeyAuth;

impl axum::extract::FromRequestParts<AppState> for MasterKeyAuth {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let master_key = state.config.general_settings.master_key.as_deref().unwrap_or("");
        if master_key.is_empty() {
            return Err((StatusCode::UNAUTHORIZED, "master key not configured"));
        }
        let auth = parts.headers.get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or((StatusCode::UNAUTHORIZED, "missing authorization"))?;
        if auth == master_key {
            Ok(MasterKeyAuth)
        } else {
            Err((StatusCode::UNAUTHORIZED, "invalid master key"))
        }
    }
}

#[derive(Serialize)]
pub struct GeneratedKey {
    key: String,
    prefix: String,
}

pub async fn generate_key(
    State(state): State<AppState>,
    _auth: MasterKeyAuth,
) -> Result<Json<GeneratedKey>, Response> {
    let db = state.db.as_ref().ok_or_else(|| {
        (StatusCode::SERVICE_UNAVAILABLE, "database not configured").into_response()
    })?;

    let random_bytes: [u8; 24] = rand::random();
    let raw = format!("sk-tc-{}", hex::encode(random_bytes));
    let hash = hex::encode(Sha256::digest(raw.as_bytes()));
    let prefix = raw[..15].to_string();

    db.insert_key(&hash, &prefix).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response()
    })?;

    Ok(Json(GeneratedKey { key: raw, prefix }))
}

pub async fn list_keys(
    State(state): State<AppState>,
    _auth: MasterKeyAuth,
) -> Result<Json<serde_json::Value>, Response> {
    let db = state.db.as_ref().ok_or_else(|| {
        (StatusCode::SERVICE_UNAVAILABLE, "database not configured").into_response()
    })?;

    let rows = db.list_keys().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response()
    })?;

    let keys: Vec<_> = rows.iter().map(|r| json!({
        "id": r.id.to_string(),
        "key_prefix": r.key_prefix,
        "name": r.name,
        "tpm_limit": r.tpm_limit,
        "rpm_limit": r.rpm_limit,
        "total_spend": r.total_spend,
        "is_active": r.is_active,
    })).collect();

    Ok(Json(json!({"object": "list", "data": keys})))
}

pub async fn delete_key(
    State(state): State<AppState>,
    _auth: MasterKeyAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, Response> {
    let db = state.db.as_ref().ok_or_else(|| {
        (StatusCode::SERVICE_UNAVAILABLE, "database not configured").into_response()
    })?;

    db.deactivate_key(id).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response()
    })?;

    Ok(Json(json!({"deleted": true, "id": id.to_string()})))
}
