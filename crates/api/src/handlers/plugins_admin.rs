//! Admin plugin management (Wave 118) and the generic
//! transaction verify endpoint used by plugin/manual gateways.
//!
//! - `GET /admin/plugins` — every scanned plugin: manifest
//!   summary, enabled flag, and the load error when one failed.
//! - `POST /admin/plugins/:id/enable` / `/disable` — persisted.
//! - `GET|PUT /admin/plugins/:id/config` — the plugin's own
//!   configuration (visible to that plugin's `config.get` only).
//! - `POST /admin/plugins/:id/invoke` — call any operation on a
//!   JS plugin (e.g. a manual gateway's `confirm_payment`).
//! - `GET /admin/plugins/:id/logs` — drain the plugin's log lines.
//! - `POST /transactions/:id/verify` — ask the gateway (native or
//!   plugin lane) to verify the transaction's provider reference
//!   and settle on a confirmed, amount-matching result.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::transaction::TxStatus;
use miraclepay_domain::webhook::WebhookEvent;

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::plugins;
use crate::state::AppState;

fn require_admin(auth: &AuthUser) -> Result<(), ApiError> {
    if auth.role == "admin" {
        Ok(())
    } else {
        Err(ApiError::Forbidden("admin only".to_string()))
    }
}

/// `GET /admin/plugins`
pub async fn list_plugins(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&auth)?;
    let plugins = state.plugins.read().await;
    let mut list: Vec<Value> = plugins
        .values()
        .map(|p| {
            json!({
                "id": p.id(),
                "name": p.manifest.as_ref().map(|m| m.name.clone()),
                "version": p.manifest.as_ref().map(|m| m.version.clone()),
                "kind": p.manifest.as_ref().map(|m| m.kind.as_str()),
                "provides": p.provides(),
                "enabled": p.enabled,
                "load_error": p.load_error,
                "config_schema": p
                    .manifest
                    .as_ref()
                    .map(|m| m.config_schema_json())
                    .unwrap_or(Value::Null),
            })
        })
        .collect();
    list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Json(json!({"plugins": list})))
}

async fn set_enabled(
    auth: &AuthUser,
    state: &AppState,
    id: &str,
    enabled: bool,
) -> Result<Json<Value>, ApiError> {
    require_admin(auth)?;
    {
        let mut plugins = state.plugins.write().await;
        let plugin = plugins
            .get_mut(id)
            .ok_or_else(|| ApiError::NotFound("plugin".to_string()))?;
        if plugin.load_error.is_some() && enabled {
            return Err(ApiError::BadRequest(format!(
                "plugin cannot be enabled: {}",
                plugin.load_error.clone().unwrap_or_default()
            )));
        }
        plugin.enabled = enabled;
    }
    state
        .plugin_enabled
        .write()
        .await
        .insert(id.to_string(), enabled);
    state.audit(
        Some(auth.user_id.clone()),
        None,
        if enabled {
            "plugin.enabled"
        } else {
            "plugin.disabled"
        },
        "plugin",
        Some(id.to_string()),
    );
    Ok(Json(json!({"ok": true, "enabled": enabled})))
}

/// `POST /admin/plugins/:id/enable`
pub async fn enable_plugin(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_enabled(&auth, &state, &id, true).await
}

/// `POST /admin/plugins/:id/disable`
pub async fn disable_plugin(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_enabled(&auth, &state, &id, false).await
}

/// `GET /admin/plugins/:id/config`
pub async fn get_plugin_config(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&auth)?;
    if !state.plugins.read().await.contains_key(&id) {
        return Err(ApiError::NotFound("plugin".to_string()));
    }
    let config = state
        .plugin_configs
        .read()
        .await
        .get(&id)
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok(Json(json!({"plugin_id": id, "config": config})))
}

/// `PUT /admin/plugins/:id/config`
pub async fn put_plugin_config(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(config): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&auth)?;
    if !config.is_object() {
        return Err(ApiError::BadRequest(
            "config must be a JSON object".to_string(),
        ));
    }
    {
        let plugins = state.plugins.read().await;
        let plugin = plugins
            .get(&id)
            .ok_or_else(|| ApiError::NotFound("plugin".to_string()))?;
        if let Some(engine) = &plugin.engine {
            engine.set_config(config.clone());
        }
    }
    state
        .plugin_configs
        .write()
        .await
        .insert(id.clone(), config);
    state.audit(
        Some(auth.user_id.clone()),
        None,
        "plugin.config_updated",
        "plugin",
        Some(id),
    );
    Ok(Json(json!({"ok": true})))
}

#[derive(Debug, Deserialize)]
pub struct InvokeReq {
    pub operation: String,
    #[serde(default)]
    pub args: Value,
}

/// `POST /admin/plugins/:id/invoke`
pub async fn invoke_plugin(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<InvokeReq>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&auth)?;
    let plugins = state.plugins.read().await;
    let plugin = plugins
        .get(&id)
        .ok_or_else(|| ApiError::NotFound("plugin".to_string()))?;
    if !plugin.enabled {
        return Err(ApiError::BadRequest("plugin is disabled".to_string()));
    }
    let engine = plugin
        .engine
        .clone()
        .ok_or_else(|| ApiError::BadRequest("plugin has no JS engine (http lane)".to_string()))?;
    let operation = req.operation.clone();
    let args = req.args.clone();
    let result = tokio::task::spawn_blocking(move || engine.call(&operation, &args))
        .await
        .map_err(|e| ApiError::BadRequest(format!("plugin task failed: {e}")))?
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(json!({"result": result})))
}

/// `GET /admin/plugins/:id/logs`
pub async fn plugin_logs(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&auth)?;
    let plugins = state.plugins.read().await;
    let plugin = plugins
        .get(&id)
        .ok_or_else(|| ApiError::NotFound("plugin".to_string()))?;
    let logs = plugin
        .engine
        .as_ref()
        .map(|e| e.drain_logs())
        .unwrap_or_default();
    Ok(Json(json!({"logs": logs})))
}

/// `POST /transactions/:id/verify` — merchant-triggered
/// re-verification. Works for native gateways (registry) and for
/// plugin-lane gateways; settles only on a confirmed result whose
/// amount matches the transaction.
pub async fn verify_transaction(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(tx_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let tx = state
        .transactions
        .read()
        .await
        .get(&tx_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound("transaction".to_string()))?;
    let is_owner = auth.role == "admin"
        || auth.merchant_id.as_deref() == Some(tx.merchant_id.as_str())
        || state
            .merchants
            .read()
            .await
            .get(&tx.merchant_id)
            .map(|m| m.owner_user_id == auth.user_id)
            .unwrap_or(false);
    if !is_owner {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    if tx.status == TxStatus::Completed {
        return Ok(Json(
            json!({"ok": true, "status": "completed", "already": true}),
        ));
    }
    let provider_ref = tx
        .provider_trx_id
        .clone()
        .ok_or_else(|| ApiError::BadRequest("transaction has no provider reference".to_string()))?;
    let registry = state.registry_for(&tx.merchant_id);
    let result = match registry.get_by_str(&tx.gateway) {
        Some(gateway) => gateway
            .verify(&provider_ref)
            .await
            .map_err(|e| ApiError::BadRequest(e.to_string()))?,
        None if plugins::plugin_provides_gateway(&state, &tx.gateway).await => {
            plugins::plugin_verify(&state, &tx.gateway, &provider_ref)
                .await
                .map_err(|e| ApiError::BadRequest(e.to_string()))?
        }
        None => {
            return Err(ApiError::BadRequest(format!(
                "unknown gateway: {}",
                tx.gateway
            )))
        }
    };
    if !result.verified {
        return Ok(Json(
            json!({"ok": true, "status": "unverified", "verified": false}),
        ));
    }
    if result.amount_minor != tx.amount.amount_minor {
        return Err(ApiError::BadRequest(format!(
            "provider amount {} does not match transaction amount {}",
            result.amount_minor, tx.amount.amount_minor
        )));
    }
    let completed = {
        let mut transactions = state.transactions.write().await;
        match transactions.get_mut(&tx_id) {
            Some(stored) => {
                stored.status = TxStatus::Completed;
                stored.provider_trx_id = Some(result.provider_trx_id.clone());
                if let Some(sender) = &result.sender {
                    stored.sender_number = Some(sender.clone());
                }
                stored.updated_at = chrono::Utc::now();
                Some(stored.clone())
            }
            None => None,
        }
    };
    if let Some(completed) = completed {
        state.webhook_events.write().await.push(WebhookEvent {
            id: miraclepay_domain::ids::webhook_event_id(),
            merchant_id: completed.merchant_id.clone(),
            event_type: "payment.completed".to_string(),
            payload: json!({
                "transaction_id": completed.id,
                "merchant_id": completed.merchant_id,
                "gateway": completed.gateway,
                "amount_minor": completed.amount.amount_minor,
                "currency": completed.amount.currency.code(),
                "status": completed.status,
                "provider_trx_id": completed.provider_trx_id,
                "reference": completed.reference,
                "via": "verify_endpoint",
            }),
            created_at: chrono::Utc::now(),
        });
        state.audit(
            Some(auth.user_id.clone()),
            Some(completed.merchant_id.clone()),
            "transaction.verified",
            "transaction",
            Some(completed.id.clone()),
        );
        return Ok(Json(
            json!({"ok": true, "status": "completed", "verified": true}),
        ));
    }
    Err(ApiError::NotFound("transaction".to_string()))
}
