//! Refresh tokens (Wave 25).
//!
//! Access JWTs live 24h. A refresh token lets a client obtain a
//! fresh pair without re-entering the password:
//!
//! - Opaque CSPRNG token (`mprt_` + 64 hex chars from OsRng),
//!   30-day lifetime.
//! - Only the SHA-256 hash is stored (same rule as API keys), so a
//!   state dump cannot leak usable tokens.
//! - **Rotation**: every successful refresh revokes the presented
//!   token and issues a new one; a replayed old token is rejected.
//! - Logout revokes the presented token.

use chrono::{DateTime, Duration, Utc};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::state::AppState;

/// Refresh-token lifetime in days.
pub const REFRESH_TTL_DAYS: i64 = 30;

/// A stored refresh token, keyed in state by its SHA-256 hash.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RefreshRecord {
    pub user_id: String,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}

/// Generate a fresh raw refresh token (CSPRNG).
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("mprt_{hex}")
}

/// SHA-256 hex of a raw token — the only form ever stored.
pub fn hash_refresh_token(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Issue and store a refresh token for `user_id`; returns the raw
/// token (shown to the client exactly once).
pub async fn issue_refresh_token(state: &AppState, user_id: &str) -> String {
    let raw = generate_refresh_token();
    let record = RefreshRecord {
        user_id: user_id.to_string(),
        expires_at: Utc::now() + Duration::days(REFRESH_TTL_DAYS),
        revoked: false,
    };
    state
        .refresh_tokens
        .write()
        .await
        .insert(hash_refresh_token(&raw), record);
    raw
}

/// Why a presented refresh token cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshError {
    Unknown,
    Revoked,
    Expired,
}

/// Validate a presented token **and rotate it**: on success the old
/// token is revoked and the user id returned; the caller issues the
/// replacement pair. Reuse of a rotated token fails as `Revoked`.
pub async fn rotate_refresh_token(state: &AppState, raw: &str) -> Result<String, RefreshError> {
    let hash = hash_refresh_token(raw);
    let mut tokens = state.refresh_tokens.write().await;
    let record = tokens.get_mut(&hash).ok_or(RefreshError::Unknown)?;
    if record.revoked {
        return Err(RefreshError::Revoked);
    }
    if record.expires_at <= Utc::now() {
        record.revoked = true;
        return Err(RefreshError::Expired);
    }
    record.revoked = true;
    Ok(record.user_id.clone())
}

/// Revoke a presented token (logout). Returns whether a live token
/// was revoked.
pub async fn revoke_refresh_token(state: &AppState, raw: &str) -> bool {
    let hash = hash_refresh_token(raw);
    let mut tokens = state.refresh_tokens.write().await;
    match tokens.get_mut(&hash) {
        Some(record) if !record.revoked => {
            record.revoked = true;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_prefixed() {
        let a = generate_refresh_token();
        let b = generate_refresh_token();
        assert_ne!(a, b);
        assert!(a.starts_with("mprt_"));
        assert_eq!(a.len(), 5 + 64);
    }

    #[test]
    fn hash_is_deterministic_and_hides_the_token() {
        let raw = generate_refresh_token();
        assert_eq!(hash_refresh_token(&raw), hash_refresh_token(&raw));
        assert!(!hash_refresh_token(&raw).contains(&raw[5..13]));
    }
}
