//! API-key authentication for the public REST API (Wave 103).
//!
//! Merchant API keys (`X-Api-Key: <raw>` or
//! `Authorization: Bearer <raw>`) authenticate as the key's
//! merchant for the endpoints that opt in, with the key's scopes
//! enforced per operation: a read-only key cannot create payments,
//! a revoked key authenticates nothing. The raw key is hashed on
//! arrival and never stored or logged — the same rule the SMS
//! device-key path has always used.

use axum::http::HeaderMap;

use miraclepay_domain::api_key::ApiScope;

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

/// SHA-256 hex of a raw API key (the domain crate's helper, shared
/// with the device-key path).
pub fn hash_key(raw: &str) -> String {
    miraclepay_domain::api_key::hash_key(raw)
}

/// Authenticate an API key from the request headers, requiring
/// `scope`. Returns an [`AuthUser`] scoped to the key's merchant
/// (role `"api_key"`), so the handlers' existing ownership checks
/// apply unchanged.
pub async fn authenticate_api_key(
    state: &AppState,
    headers: &HeaderMap,
    scope: ApiScope,
) -> Result<AuthUser, ApiError> {
    let raw = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .filter(|v| v.starts_with("mp_"))
                .map(str::to_string)
        })
        .ok_or(ApiError::Unauthorized)?;
    let key_hash = hash_key(raw.trim());
    let key = state
        .api_keys
        .read()
        .await
        .get(&key_hash)
        .cloned()
        .ok_or(ApiError::Unauthorized)?;
    if key.revoked || !key.allows(scope) {
        return Err(ApiError::Unauthorized);
    }
    if let Some(stored) = state.api_keys.write().await.get_mut(&key_hash) {
        stored.last_used_at = Some(chrono::Utc::now());
    }
    Ok(AuthUser {
        user_id: key.id.clone(),
        role: "api_key".to_string(),
        merchant_id: Some(key.merchant_id.clone()),
    })
}
