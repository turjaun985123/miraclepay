//! API key HTTP handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation built on Axum 0.7.
//!
//! Endpoints (wired up by the router in `crate::routes`):
//!
//! * `POST   /api-keys`          — [`create_api_key`]
//! * `GET    /api-keys`          — [`list_api_keys`]
//! * `DELETE /api-keys/{id}`     — [`revoke_api_key`]
//!
//! An API key authenticates a merchant's server-side integration.
//! Only a hash of the raw key is ever stored (the `api_keys` map in
//! [`AppState`](crate::state::AppState) is keyed by that hash); the
//! raw key itself is returned exactly once, by [`create_api_key`],
//! and can never be recovered afterwards — see the comment there.
//!
//! Ownership rule shared by every handler in this module: the target
//! merchant must belong to the caller (`merchant.owner_user_id ==
//! auth.user_id`), unless the caller is a platform admin. Creating
//! and listing distinguish "no such merchant" (`404`) from "not your
//! merchant" (`403 Forbidden("not your merchant")`); revoking resolves
//! the key first and then applies the same rule to its merchant.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use miraclepay_domain::{
    api_key::{generate_key, ApiKey, ApiScope},
    ids::api_key_id,
    merchant::Merchant,
};

use crate::{auth_middleware::AuthUser, dto::ApiError, state::AppState};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for [`create_api_key`].
#[derive(Debug, Clone, Deserialize)]
pub struct CreateReq {
    /// Merchant the new key will belong to. Must be owned by the
    /// caller (or the caller must be an admin).
    pub merchant_id: String,
    /// Human-readable name for the key, e.g. `"Production server"`.
    pub name: String,
    /// Scope names to grant, e.g. `["payments_read", "payments_write"]`.
    /// Each entry must be one of the names accepted by
    /// [`parse_scope`]; anything else is a `400 Bad Request`.
    pub scopes: Vec<String>,
}

/// Query parameters for [`list_api_keys`].
#[derive(Debug, Clone, Deserialize)]
pub struct ListQuery {
    /// Merchant whose keys should be listed. Must be owned by the
    /// caller (or the caller must be an admin).
    pub merchant_id: String,
}

/// Response body for [`create_api_key`] — the only response in the
/// whole API that contains a raw key.
#[derive(Debug, Clone, Serialize)]
pub struct CreatedApiKey {
    /// Unique API key identifier.
    pub id: String,
    /// Human-readable name given to the key.
    pub name: String,
    /// The raw API key. Shown ONLY here, at creation time; it is
    /// never stored and cannot be shown again.
    pub key: String,
    /// Public, non-secret prefix of the raw key, safe to display.
    pub key_prefix: String,
    /// Permissions granted to the key.
    pub scopes: Vec<ApiScope>,
}

/// Public view of a stored [`ApiKey`], used by [`list_api_keys`] and
/// [`revoke_api_key`].
///
/// Deliberately has no `key_hash` field: the hash is a credential
/// equivalent (anyone holding it could be compared against candidate
/// keys offline) and is never exposed through the API.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyView {
    /// Unique API key identifier.
    pub id: String,
    /// Human-readable name given to the key.
    pub name: String,
    /// Public, non-secret prefix of the raw key, safe to display.
    pub key_prefix: String,
    /// Permissions granted to the key.
    pub scopes: Vec<ApiScope>,
    /// Whether the key has been revoked.
    pub revoked: bool,
    /// When the key was last used successfully, if ever.
    pub last_used_at: Option<DateTime<Utc>>,
    /// When the key record was created.
    pub created_at: DateTime<Utc>,
}

impl From<&ApiKey> for ApiKeyView {
    fn from(key: &ApiKey) -> Self {
        Self {
            id: key.id.clone(),
            name: key.name.clone(),
            key_prefix: key.key_prefix.clone(),
            scopes: key.scopes.clone(),
            revoked: key.revoked,
            last_used_at: key.last_used_at,
            created_at: key.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses one scope name from a request body into an [`ApiScope`].
///
/// Accepted names are exactly the `snake_case` spellings of the
/// domain enum: `"payments_read"`, `"payments_write"`,
/// `"invoices_write"`, `"webhooks_manage"`, `"refunds_write"`, and
/// `"full_access"`. Anything else is a `400 Bad Request` naming the
/// offending value, so a typo fails loudly instead of silently
/// granting a narrower (or wider) key than intended.
fn parse_scope(raw: &str) -> Result<ApiScope, ApiError> {
    match raw {
        "payments_read" => Ok(ApiScope::PaymentsRead),
        "payments_write" => Ok(ApiScope::PaymentsWrite),
        "invoices_write" => Ok(ApiScope::InvoicesWrite),
        "webhooks_manage" => Ok(ApiScope::WebhooksManage),
        "refunds_write" => Ok(ApiScope::RefundsWrite),
        "full_access" => Ok(ApiScope::FullAccess),
        other => Err(ApiError::BadRequest(format!(
            "unknown api key scope: {other}"
        ))),
    }
}

/// Loads the merchant `merchant_id` and enforces this module's
/// ownership rule: the caller must own it, or be a platform admin.
///
/// * Unknown id → `404 Not Found`.
/// * Owned by someone else → `403 Forbidden("not your merchant")`.
async fn owned_merchant(
    state: &AppState,
    auth: &AuthUser,
    merchant_id: &str,
) -> Result<Merchant, ApiError> {
    let merchant = state
        .merchants
        .read()
        .await
        .get(merchant_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("merchant {merchant_id} not found")))?;

    if merchant.owner_user_id != auth.user_id && !auth.is_admin() {
        return Err(ApiError::Forbidden("not your merchant".to_string()));
    }

    Ok(merchant)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /api-keys` — create an API key for a merchant the caller owns.
///
/// Generates a fresh key with the domain generator, stores only its
/// hash (the `api_keys` map is keyed by that hash), and returns the
/// raw key in the `201 Created` response.
///
/// Responds `404` for an unknown merchant, `403` for a merchant the
/// caller does not own, and `400` for an unknown scope name.
pub async fn create_api_key(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateReq>,
) -> Result<(StatusCode, Json<CreatedApiKey>), ApiError> {
    let merchant = owned_merchant(&state, &auth, &req.merchant_id).await?;

    // Parse every scope before generating anything, so a bad scope
    // name rejects the whole request and no key is created.
    let scopes: Vec<ApiScope> = req
        .scopes
        .iter()
        .map(|scope| parse_scope(scope))
        .collect::<Result<_, _>>()?;

    let (raw, prefix, hash) = generate_key();

    let key = ApiKey {
        id: api_key_id(),
        merchant_id: merchant.id.clone(),
        name: req.name.clone(),
        key_prefix: prefix.clone(),
        key_hash: hash.clone(),
        scopes: scopes.clone(),
        last_used_at: None,
        revoked: false,
        created_at: Utc::now(),
    };

    // Keyed by key_hash: the raw key is never stored anywhere.
    state.api_keys.write().await.insert(hash, key.clone());

    state.audit(
        Some(auth.user_id.clone()),
        Some(merchant.id.clone()),
        "api_key.created",
        "api_key",
        Some(key.id.clone()),
    );

    // IMPORTANT: `raw` is returned here and ONLY here. This response
    // is the one and only time the raw key is ever shown — only its
    // hash and prefix are stored, so it cannot be recovered or
    // displayed again. The caller must copy it now.
    Ok((
        StatusCode::CREATED,
        Json(CreatedApiKey {
            id: key.id,
            name: key.name,
            key: raw,
            key_prefix: prefix,
            scopes,
        }),
    ))
}

/// `GET /api-keys?merchant_id=...` — list one owned merchant's keys.
///
/// Returns [`ApiKeyView`]s, which never include `key_hash` (or,
/// obviously, the raw key). Results are ordered by creation time so
/// the response is stable between calls. Responds `404` for an
/// unknown merchant and `403` for one the caller does not own.
pub async fn list_api_keys(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<ApiKeyView>>, ApiError> {
    let merchant = owned_merchant(&state, &auth, &query.merchant_id).await?;

    let keys = state.api_keys.read().await;

    let mut views: Vec<ApiKeyView> = keys
        .values()
        .filter(|key| key.merchant_id == merchant.id)
        .map(ApiKeyView::from)
        .collect();
    views.sort_by_key(|view| view.created_at);

    Ok(Json(views))
}

/// `DELETE /api-keys/{id}` — revoke one API key.
///
/// The `api_keys` map is keyed by key hash, not by id, so the key is
/// found by scanning the stored values for a matching `id`. The
/// ownership rule is then applied to that key's merchant: only its
/// owner (or an admin) may revoke it. Revocation is a flag flip, not
/// a deletion — the record stays so past use remains auditable, and
/// the domain's `ApiKey::allows` rejects revoked keys.
///
/// Responds `404` when no key (or no merchant) matches, `403` when
/// the key belongs to someone else's merchant, and with the updated
/// [`ApiKeyView`] on success.
pub async fn revoke_api_key(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiKeyView>, ApiError> {
    // Resolve the key's merchant first (the map is keyed by hash, so
    // this is a scan by id), so ownership can be checked before any
    // mutation happens.
    let merchant_id = {
        let keys = state.api_keys.read().await;
        keys.values()
            .find(|key| key.id == id)
            .map(|key| key.merchant_id.clone())
            .ok_or_else(|| ApiError::NotFound(format!("api key {id} not found")))?
    };

    owned_merchant(&state, &auth, &merchant_id).await?;

    let view = {
        let mut keys = state.api_keys.write().await;
        let key = keys
            .values_mut()
            .find(|key| key.id == id)
            .ok_or_else(|| ApiError::NotFound(format!("api key {id} not found")))?;
        key.revoked = true;
        ApiKeyView::from(&*key)
    };

    state.audit(
        Some(auth.user_id.clone()),
        Some(merchant_id),
        "api_key.revoked",
        "api_key",
        Some(id),
    );

    Ok(Json(view))
}
