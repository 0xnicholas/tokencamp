use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use sha2::{Sha256, Digest};
use std::sync::Arc;
use tokencamp_core::{AuthContext, ChatRequest, ModelPricing, ProviderError, ProxyHook};

use crate::resilience::retry;
use crate::auth::KeyAuth;
use crate::{error::AppError, extractors::ValidJson, AppState};

pub async fn chat_completions(
    State(state): State<AppState>,
    _auth: KeyAuth,
    ValidJson(request): ValidJson<ChatRequest>,
) -> Result<Response, AppError> {
    let stream = request.stream.unwrap_or(false);
    let model = request.model.clone();

    // ---- Auth ----
    let auth_ctx = authenticate(&state, &_auth.api_key).await?;

    // ---- Pre-call hooks ----
    for hook in state.hooks.iter() {
        hook.async_pre_call_hook(&request, &auth_ctx).await?;
    }

    // ---- Resolve provider ----
    let (provider_config, api_key) = state.resolve_provider(&model, &request).await?;
    let provider: Arc<dyn tokencamp_core::ProviderConfig> = provider_config.into();
    let deployment_id = format!("{}:{}", "resolved", &model);

    // ---- Call ----
    let result = if stream {
        let handler = state.handler.clone();
        let wrapper = retry_with_cooldown(
            &state, &deployment_id,
            || {
                let provider = provider.clone();
                let request = request.clone();
                let api_key = api_key.clone();
                let handler = handler.clone();
                async move { handler.complete_stream_owned(provider, request, api_key).await }
            },
        ).await?;

        // Streaming: post hooks after stream (simplified — triggers on stream end)
        let sse_stream = wrapper.map(move |chunk| {
            let json = match chunk {
                Ok(c) => serde_json::to_string(&c).unwrap(),
                Err(e) => serde_json::json!({"error": {"message": e.to_string()}}).to_string(),
            };
            Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(json))
        });
        return Ok(axum::response::Sse::new(sse_stream).into_response());
    } else {
        retry_with_cooldown(
            &state, &deployment_id,
            || {
                let request = request.clone();
                let api_key = api_key.clone();
                let handler = state.handler.clone();
                let provider = provider.clone();
                async move { handler.complete(&request, provider.as_ref(), &api_key).await }
            },
        ).await?
    };

    // ---- Post-call hooks (spawn async) ----
    let hooks = state.hooks.clone();
    let req = request.clone();
    let auth = auth_ctx.clone();
    let resp = result.clone();
    tokio::spawn(async move {
        for hook in hooks.iter() {
            hook.async_post_call_hook(&req, &resp, &auth).await;
        }
    });

    Ok(Json(result).into_response())
}

/// 认证用户请求，返回 AuthContext
async fn authenticate(state: &AppState, api_key: &str) -> Result<AuthContext, AppError> {
    if api_key.is_empty() {
        return Err(AppError::Provider(ProviderError::UpstreamError {
            status: 401, message: "missing api key".into(),
        }));
    }

    let hash = hex::encode(Sha256::digest(api_key.as_bytes()));
    let cache_key = format!("key:{}", hash);

    // 1. DualCache
    let cache_ref: &dyn tokencamp_core::CacheLayer = state.cache.as_ref();
    if let Some(cached) = cache_ref.get(&cache_key).await {
        return deserialize_auth(&cached);
    }

    // 2. PostgreSQL
    if let Some(ref db) = state.db {
        if let Some(row) = db.find_key_by_hash(&hash).await {
            let ctx = AuthContext {
                key_id: row.id.to_string(),
                key_name: row.name.clone(),
                tpm_limit: row.tpm_limit.map(|v| v as u32),
                rpm_limit: row.rpm_limit.map(|v| v as u32),
                model_pricing: None, // filled later by resolve_provider
            };
            let json = serde_json::to_string(&ctx).unwrap_or_default();
            cache_ref.set(&cache_key, &json, std::time::Duration::from_secs(300)).await;
            return Ok(ctx);
        }
    }

    // 3. Fallback: YAML 列表
    if state.config.auth.api_keys.iter().any(|k| k == api_key) {
        return Ok(AuthContext {
            key_id: "yaml-static".into(),
            key_name: None,
            tpm_limit: None,
            rpm_limit: None,
            model_pricing: None,
        });
    }

    Err(AppError::Provider(ProviderError::UpstreamError {
        status: 401, message: "invalid api key".into(),
    }))
}

fn deserialize_auth(json: &str) -> Result<AuthContext, AppError> {
    #[derive(serde::Deserialize)]
    struct Cached { key_id: String, key_name: Option<String>, tpm_limit: Option<u32>, rpm_limit: Option<u32> }
    let c: Cached = serde_json::from_str(json).map_err(|_| AppError::Provider(ProviderError::UpstreamError {
        status: 500, message: "cache corrupt".into(),
    }))?;
    Ok(AuthContext { key_id: c.key_id, key_name: c.key_name, tpm_limit: c.tpm_limit, rpm_limit: c.rpm_limit, model_pricing: None })
}

async fn retry_with_cooldown<T, F, Fut>(
    state: &AppState, deployment_id: &str, f: F,
) -> Result<T, AppError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, ProviderError>>,
{
    let max_retries = state.retry_config.num_retries;
    let base = state.retry_config.retry_after;
    let allowed_fails = state.config.router_settings.allowed_fails;
    let mut last_err = None;

    for attempt in 0..=max_retries {
        match f().await {
            Ok(val) => { state.cooldown.record_success(deployment_id).await; return Ok(val); }
            Err(e) if retry::is_retryable(&e) && attempt < max_retries => {
                last_err = Some(e);
                tokio::time::sleep(retry::retry_delay(attempt, base)).await;
            }
            Err(e) => { last_err = Some(e); break; }
        }
    }

    let failures = state.cooldown.record_failure(deployment_id).await;
    if failures >= allowed_fails {
        let _ = state.cooldown.mark(deployment_id, state.config.router_settings.cooldown_time).await;
    }
    Err(AppError::Provider(last_err.unwrap_or(ProviderError::Timeout)))
}
