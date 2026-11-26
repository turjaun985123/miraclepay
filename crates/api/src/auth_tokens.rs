//! One-time auth tokens (Waves 88–89): password reset and email
//! verification share one store and one lifecycle.
//!
//! Tokens are opaque CSPRNG strings (`mpat_` + 64 hex chars);
//! only their SHA-256 hash is stored (the same rule as refresh
//! tokens and API keys), each token is single-use, and expiry is
//! enforced at consume time. Purposes are namespaced so a reset
//! token can never be spent as a verification token or vice versa.

use chrono::{DateTime, Duration, Utc};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::refresh::hash_refresh_token;
use crate::state::AppState;

/// Token purpose: password reset.
pub const PURPOSE_PASSWORD_RESET: &str = "password_reset";
/// Token purpose: email verification.
pub const PURPOSE_EMAIL_VERIFY: &str = "email_verify";

/// A stored one-time token, keyed in state by its hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthTokenRecord {
    pub purpose: String,
    pub user_id: String,
    pub expires_at: DateTime<Utc>,
}

/// Issue a token for `purpose`/`user_id`; returns the raw token
/// (shown to the user once, via email) — only its hash is stored.
pub async fn issue(state: &AppState, purpose: &str, user_id: &str, ttl: Duration) -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let raw = format!("mpat_{hex}");
    state.auth_tokens.write().await.insert(
        hash_refresh_token(&raw),
        AuthTokenRecord {
            purpose: purpose.to_string(),
            user_id: user_id.to_string(),
            expires_at: Utc::now() + ttl,
        },
    );
    raw
}

/// Consume a token: valid + unexpired + right purpose → the user
/// id, and the token is deleted (single-use). Any failure → None.
pub async fn consume(state: &AppState, purpose: &str, raw: &str) -> Option<String> {
    let hash = hash_refresh_token(raw);
    let mut tokens = state.auth_tokens.write().await;
    let record = tokens.get(&hash)?;
    if record.purpose != purpose || record.expires_at <= Utc::now() {
        tokens.remove(&hash);
        return None;
    }
    let user_id = record.user_id.clone();
    tokens.remove(&hash);
    Some(user_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn issue_consume_round_trip_and_single_use() {
        let state = AppState::new("s".to_string());
        let raw = issue(&state, PURPOSE_PASSWORD_RESET, "usr_1", Duration::hours(1)).await;
        assert!(raw.starts_with("mpat_"));
        // Wrong purpose cannot spend it (and burns it).
        assert!(consume(&state, PURPOSE_EMAIL_VERIFY, &raw).await.is_none());
        let raw2 = issue(&state, PURPOSE_PASSWORD_RESET, "usr_1", Duration::hours(1)).await;
        assert_eq!(
            consume(&state, PURPOSE_PASSWORD_RESET, &raw2).await,
            Some("usr_1".to_string())
        );
        assert!(consume(&state, PURPOSE_PASSWORD_RESET, &raw2)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn expired_tokens_do_not_consume() {
        let state = AppState::new("s".to_string());
        let raw = issue(&state, PURPOSE_EMAIL_VERIFY, "usr_1", Duration::seconds(-5)).await;
        assert!(consume(&state, PURPOSE_EMAIL_VERIFY, &raw).await.is_none());
    }
}
