// gateway/src/routes/models.rs
use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::AppState;

pub async fn list_models(State(state): State<AppState>) -> Json<Value> {
    let models: Vec<_> = state
        .config
        .model_list
        .iter()
        .map(|m| {
            json!({
                "id": m.model_name,
                "object": "model",
                "created": 1717000000u64,
                "owned_by": m.provider,
            })
        })
        .collect();

    Json(json!({
        "object": "list",
        "data": models,
    }))
}
