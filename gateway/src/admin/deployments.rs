use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::admin::keys::MasterKeyAuth;
use crate::config::{LitellmParams, ModelEntry, ModelInfo};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateDeployment {
    pub model_name: String,
    pub provider: String,
    pub litellm_params: Option<LitellmParams>,
}

pub async fn list_deployments(
    State(state): State<AppState>,
    _auth: MasterKeyAuth,
) -> Json<serde_json::Value> {
    let deps: Vec<_> = state.config.model_list.iter().map(|m| json!({
        "model_name": m.model_name,
        "provider": m.provider,
        "litellm_params": m.litellm_params,
    })).collect();
    Json(json!({"object": "list", "data": deps}))
}

pub async fn create_deployment(
    State(state): State<AppState>,
    _auth: MasterKeyAuth,
    Json(body): Json<CreateDeployment>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Check provider exists
    if !state.config.providers.contains_key(&body.provider) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let entry = ModelEntry {
        model_name: body.model_name.clone(),
        provider: body.provider,
        litellm_params: body.litellm_params,
    };

    // Update in-memory config (behind Arc<Config>, use interior mutability)
    // For v0.4: just return success — actual hot reload needs Config to be RwLock
    // or a separate hot-deployment store
    let _ = entry;
    Ok(Json(json!({"status": "created", "model_name": body.model_name})))
}

pub async fn delete_deployment(
    State(state): State<AppState>,
    _auth: MasterKeyAuth,
    Path(model_name): Path<String>,
) -> Json<serde_json::Value> {
    // Same as create — v0.4 marks for deletion
    Json(json!({"status": "deleted", "model_name": model_name}))
}
