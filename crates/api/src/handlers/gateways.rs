//! Gateway catalogue and per-merchant gateway configuration
//! handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! Endpoints:
//!
//! * `list_gateways` — public catalogue of every gateway the
//!   platform supports (all fourteen), with no credentials or
//!   per-merchant state attached.
//! * `list_merchant_gateways` — the calling merchant owner's stored
//!   configurations, with API secrets masked.
//! * `upsert_gateway_config` — create or replace one gateway
//!   configuration for a merchant the caller owns.
//! * `delete_gateway_config` — remove one gateway configuration.
//! * `gateway_status` — which gateways are configured (usable) and
//!   which are not, for a merchant the caller owns.
//!
//! Secrets are write-only through this module: a stored
//! `api_secret` is never returned. Responses render it as `"***"`
//! when one is present and as `null` when none is stored.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use miraclepay_gateways::types::{GatewayConfig, GatewayId};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

/// Placeholder shown in place of a stored API secret in responses.
const MASKED_SECRET: &str = "***";

// ---------------------------------------------------------------------
// Response shapes
// ---------------------------------------------------------------------

/// One entry of the public gateway catalogue.
#[derive(Debug, Clone, Serialize)]
pub struct GatewaySummary {
    /// Stable machine-readable gateway identifier, e.g. `"bkash"`.
    pub id: GatewayId,
    /// Human-readable brand name, e.g. `"bKash"`.
    pub display_name: String,
    /// Primary operating country (`"BD"`, `"IN"`, `"PK"`), or
    /// `"GLOBAL"` for gateways not tied to one country.
    pub country: String,
    /// Whether the platform itself carries a global configuration
    /// for this gateway. MiraclePay v0.0.1 has no platform-level
    /// gateway credentials — every merchant configures their own —
    /// so this is always `false`.
    pub configured_globally: bool,
}

/// A merchant's stored gateway configuration, safe to return: the
/// API secret is masked (see [`MASKED_SECRET`]).
#[derive(Debug, Clone, Serialize)]
pub struct GatewayConfigView {
    /// Which gateway this configuration belongs to.
    pub gateway: GatewayId,
    /// Merchant / agent account number at the provider, if any.
    pub merchant_number: Option<String>,
    /// API key / public credential, if any.
    pub api_key: Option<String>,
    /// `"***"` when a secret is stored, `null` when none is. The
    /// real secret is never returned.
    pub api_secret: Option<String>,
    /// Gateway-specific extra settings, as free-form JSON.
    pub extra: serde_json::Value,
    /// Whether calls go to the provider's sandbox environment.
    pub sandbox: bool,
    /// Whether this gateway is enabled for the merchant.
    pub enabled: bool,
    /// Whether the merchant's registry reports this gateway as
    /// configured (enabled, with the credentials it needs) and
    /// therefore able to take payments.
    pub configured: bool,
}

/// Configured / unconfigured summary for one merchant.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayStatus {
    /// Ids of gateways configured and usable for the merchant, in
    /// canonical [`GatewayId::all`] order.
    pub configured: Vec<GatewayId>,
    /// Ids of gateways not (yet) configured for the merchant, in
    /// canonical [`GatewayId::all`] order.
    pub unconfigured: Vec<GatewayId>,
}

impl GatewayConfigView {
    /// Build the response view of a stored `config`, masking its
    /// secret. `configured` is the registry-computed flag for this
    /// config's gateway.
    fn masked(config: &GatewayConfig, configured: bool) -> Self {
        Self {
            gateway: config.gateway,
            merchant_number: config.merchant_number.clone(),
            api_key: config.api_key.clone(),
            api_secret: config
                .api_secret
                .as_ref()
                .map(|_| MASKED_SECRET.to_string()),
            extra: config.extra.clone(),
            sandbox: config.sandbox,
            enabled: config.enabled,
            configured,
        }
    }
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Ensure `merchant_id` exists and is owned by the authenticated
/// user. Returns `404` when the merchant does not exist and `403`
/// when it belongs to someone else.
async fn ensure_owned_merchant(
    state: &AppState,
    auth: &AuthUser,
    merchant_id: &str,
) -> Result<(), ApiError> {
    let merchants = state.merchants.read().await;
    let merchant = merchants
        .get(merchant_id)
        .ok_or_else(|| ApiError::NotFound(format!("merchant {merchant_id} not found")))?;
    if merchant.owner_user_id != auth.user_id {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    Ok(())
}

/// Snapshot of the merchant's stored gateway configurations
/// (empty when the merchant has none yet).
async fn merchant_configs(state: &AppState, merchant_id: &str) -> Vec<GatewayConfig> {
    state
        .gateway_configs
        .read()
        .await
        .get(merchant_id)
        .cloned()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------

/// List every gateway supported by the platform.
///
/// Public (no authentication): this is catalogue data only — ids,
/// display names and countries — and carries no merchant state.
/// `configured_globally` is always `false` in v0.0.1 because
/// gateway credentials are per-merchant, never platform-global.
pub async fn list_gateways() -> Json<Vec<GatewaySummary>> {
    Json(
        GatewayId::all()
            .iter()
            .map(|gateway| GatewaySummary {
                id: *gateway,
                display_name: gateway.display_name().to_string(),
                country: gateway.country().to_string(),
                configured_globally: false,
            })
            .collect(),
    )
}

/// List the gateway configurations stored for a merchant the
/// caller owns, with each API secret masked and a `configured`
/// flag computed from the merchant's gateway registry.
pub async fn list_merchant_gateways(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
) -> Result<Json<Vec<GatewayConfigView>>, ApiError> {
    ensure_owned_merchant(&state, &auth, &merchant_id).await?;

    let configs = merchant_configs(&state, &merchant_id).await;
    let configured_ids = state.registry_for(&merchant_id).configured_ids();

    Ok(Json(
        configs
            .iter()
            .map(|config| {
                GatewayConfigView::masked(config, configured_ids.contains(&config.gateway))
            })
            .collect(),
    ))
}

/// Create or replace the configuration for one gateway of a
/// merchant the caller owns.
///
/// The gateway is identified by the `gateway` field of the body,
/// which deserializes as [`GatewayId`]: an unknown identifier is
/// rejected with `400` by Axum's JSON extractor before this handler
/// runs. An existing configuration for the same gateway is
/// replaced in place; otherwise the new configuration is appended.
///
/// The response is the stored configuration with its API secret
/// masked — the secret itself is never returned.
pub async fn upsert_gateway_config(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
    Json(config): Json<GatewayConfig>,
) -> Result<Json<GatewayConfigView>, ApiError> {
    ensure_owned_merchant(&state, &auth, &merchant_id).await?;

    let gateway = config.gateway;
    {
        let mut all_configs = state.gateway_configs.write().await;
        let configs = all_configs.entry(merchant_id.clone()).or_default();
        if let Some(existing) = configs.iter_mut().find(|c| c.gateway == gateway) {
            *existing = config;
        } else {
            configs.push(config);
        }
    }

    state.audit(
        Some(auth.user_id.clone()),
        Some(merchant_id.clone()),
        "gateway.configured",
        "gateway",
        Some(gateway.as_str().to_string()),
    );

    let stored = merchant_configs(&state, &merchant_id)
        .await
        .into_iter()
        .find(|c| c.gateway == gateway)
        .ok_or_else(|| {
            ApiError::NotFound(format!("gateway {} not configured", gateway.as_str()))
        })?;
    let configured = state
        .registry_for(&merchant_id)
        .configured_ids()
        .contains(&gateway);

    Ok(Json(GatewayConfigView::masked(&stored, configured)))
}

/// Delete the configuration for one gateway of a merchant the
/// caller owns. Returns `404` when the merchant has no
/// configuration stored for that gateway (including when the
/// gateway identifier is unknown).
pub async fn delete_gateway_config(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((merchant_id, gateway_str)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    ensure_owned_merchant(&state, &auth, &merchant_id).await?;

    let gateway = GatewayId::from_str(&gateway_str)
        .ok_or_else(|| ApiError::NotFound(format!("gateway {gateway_str} not configured")))?;

    let mut all_configs = state.gateway_configs.write().await;
    let configs = all_configs.get_mut(&merchant_id).ok_or_else(|| {
        ApiError::NotFound(format!("gateway {} not configured", gateway.as_str()))
    })?;
    let before = configs.len();
    configs.retain(|c| c.gateway != gateway);
    if configs.len() == before {
        return Err(ApiError::NotFound(format!(
            "gateway {} not configured",
            gateway.as_str()
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Report which gateways are configured (usable) and which are
/// unconfigured for a merchant the caller owns, based on the
/// merchant's gateway registry.
pub async fn gateway_status(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
) -> Result<Json<GatewayStatus>, ApiError> {
    ensure_owned_merchant(&state, &auth, &merchant_id).await?;

    let configured = state.registry_for(&merchant_id).configured_ids();
    let unconfigured = GatewayId::all()
        .iter()
        .copied()
        .filter(|gateway| !configured.contains(gateway))
        .collect();

    Ok(Json(GatewayStatus {
        configured,
        unconfigured,
    }))
}
