//! Bearer-token authentication for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! Minimal HS256 JSON Web Tokens, implemented by hand — no JWT
//! crate. A token is
//!
//! ```text
//! base64url(header) "." base64url(payload) "." base64url(signature)
//! ```
//!
//! where `header` is `{"alg":"HS256","typ":"JWT"}`, `payload` is a
//! [`Claims`] document, and `signature` is the raw (not hex) bytes
//! of `HMAC-SHA256(secret, header "." payload)`. All base64url
//! encoding is unpadded, per RFC 7515.
//!
//! [`AuthUser`] is an axum extractor: a handler that takes an
//! `AuthUser` argument only runs when the request carried a valid,
//! unexpired `Authorization: Bearer <token>` header signed with
//! `AppState::jwt_secret`; anything else is rejected with
//! [`ApiError::Unauthorized`].

use axum::{extract::FromRequestParts, http::request::Parts};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::dto::ApiError;
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

/// How long a minted token stays valid, in seconds (24 hours).
const TOKEN_TTL_SECS: i64 = 24 * 60 * 60;

/// The fixed JOSE header of every token minted here.
const HEADER_JSON: &str = r#"{"alg":"HS256","typ":"JWT"}"#;

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

/// The payload of a MiraclePay token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: the user id the token was minted for.
    pub sub: String,
    /// Role string, e.g. `"admin"`, `"merchant"`, `"staff"`,
    /// `"customer"`.
    pub role: String,
    /// Merchant the token is scoped to, when it is scoped to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant_id: Option<String>,
    /// Expiry, as seconds since the Unix epoch.
    pub exp: i64,
}

// ---------------------------------------------------------------------------
// base64url (hand-rolled, no padding)
// ---------------------------------------------------------------------------

const BASE64URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes as unpadded base64url.
pub fn base64url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        out.push(BASE64URL_ALPHABET[(n >> 18) as usize & 0x3f] as char);
        out.push(BASE64URL_ALPHABET[(n >> 12) as usize & 0x3f] as char);
        if chunk.len() > 1 {
            out.push(BASE64URL_ALPHABET[(n >> 6) as usize & 0x3f] as char);
        }
        if chunk.len() > 2 {
            out.push(BASE64URL_ALPHABET[n as usize & 0x3f] as char);
        }
    }
    out
}

fn base64url_value(byte: u8) -> Option<u32> {
    match byte {
        b'A'..=b'Z' => Some(u32::from(byte - b'A')),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

/// Decode unpadded base64url.
///
/// Returns [`ApiError::Unauthorized`]: this helper is only used on
/// token material, where malformed input is an authentication
/// failure rather than a caller error worth distinguishing.
pub fn base64url_decode(input: &str) -> Result<Vec<u8>, ApiError> {
    let bytes = input.as_bytes();
    // A lone trailing sextet cannot encode a whole byte.
    if bytes.len() % 4 == 1 {
        return Err(ApiError::Unauthorized);
    }
    let mut values = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        values.push(base64url_value(byte).ok_or(ApiError::Unauthorized)?);
    }
    let mut out = Vec::with_capacity(values.len() / 4 * 3 + 3);
    for chunk in values.chunks(4) {
        let n = chunk.iter().fold(0u32, |acc, &v| (acc << 6) | v) << (6 * (4 - chunk.len()));
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Signing / minting
// ---------------------------------------------------------------------------

/// Raw HMAC-SHA256 signature bytes over a signing input.
fn sign(signing_input: &str, secret: &str) -> Vec<u8> {
    // HMAC accepts a key of any length, so this cannot fail.
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(signing_input.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Mint a token for already-built [`Claims`]. Internal so tests can
/// mint tokens [`create_token_for`] never would — e.g. expired ones.
fn mint_token(claims: &Claims, secret: &str) -> String {
    let header = base64url_encode(HEADER_JSON.as_bytes());
    let payload = base64url_encode(
        serde_json::to_string(claims)
            .expect("Claims serialisation cannot fail")
            .as_bytes(),
    );
    let signing_input = format!("{header}.{payload}");
    let signature = base64url_encode(&sign(&signing_input, secret));
    format!("{signing_input}.{signature}")
}

/// Mint a token for `user_id` with `role`, expiring 24 hours from
/// now, not scoped to any merchant.
pub fn create_token(user_id: &str, role: &str, secret: &str) -> String {
    create_token_for(user_id, role, None, secret)
}

/// Mint a token for `user_id` with `role`, expiring 24 hours from
/// now, optionally scoped to `merchant_id`.
pub fn create_token_for(
    user_id: &str,
    role: &str,
    merchant_id: Option<String>,
    secret: &str,
) -> String {
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.to_string(),
        merchant_id,
        exp: chrono::Utc::now().timestamp() + TOKEN_TTL_SECS,
    };
    mint_token(&claims, secret)
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// Verify a token against `secret` and return its [`Claims`].
///
/// The signature is recomputed and compared in constant time via
/// [`Mac::verify_slice`] on the decoded raw signature bytes; the
/// payload is only trusted after that check passes. An expired
/// token, a malformed token, a wrong signature, and a header that
/// does not declare HS256 are all [`ApiError::Unauthorized`].
pub fn verify_token(token: &str, secret: &str) -> Result<Claims, ApiError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(ApiError::Unauthorized);
    }
    let (header_b64, payload_b64, signature_b64) = (parts[0], parts[1], parts[2]);

    // The header must declare the one algorithm this module signs
    // with; never let the token choose its own algorithm.
    let header_bytes = base64url_decode(header_b64)?;
    let header: serde_json::Value =
        serde_json::from_slice(&header_bytes).map_err(|_| ApiError::Unauthorized)?;
    if header.get("alg").and_then(serde_json::Value::as_str) != Some("HS256") {
        return Err(ApiError::Unauthorized);
    }

    let signature = base64url_decode(signature_b64)?;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(format!("{header_b64}.{payload_b64}").as_bytes());
    // Constant-time comparison of the recomputed MAC against the
    // decoded signature bytes.
    mac.verify_slice(&signature)
        .map_err(|_| ApiError::Unauthorized)?;

    let payload_bytes = base64url_decode(payload_b64)?;
    let claims: Claims =
        serde_json::from_slice(&payload_bytes).map_err(|_| ApiError::Unauthorized)?;

    if claims.exp <= chrono::Utc::now().timestamp() {
        return Err(ApiError::Unauthorized);
    }
    Ok(claims)
}

// ---------------------------------------------------------------------------
// Authenticated user + axum extractor
// ---------------------------------------------------------------------------

/// The authenticated caller, extracted from a verified token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    pub user_id: String,
    pub role: String,
    pub merchant_id: Option<String>,
}

impl AuthUser {
    /// Whether this caller carries the platform `"admin"` role.
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

#[axum::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("authorization")
            .ok_or(ApiError::Unauthorized)?;
        let value = header.to_str().map_err(|_| ApiError::Unauthorized)?;
        let token = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .ok_or(ApiError::Unauthorized)?;
        let claims = verify_token(token.trim(), &state.jwt_secret)?;
        Ok(AuthUser {
            user_id: claims.sub,
            role: claims.role,
            merchant_id: claims.merchant_id,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-do-not-use-in-production";

    #[test]
    fn base64url_roundtrip_for_every_length() {
        let cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            vec![0x66],
            vec![0x66, 0x6f],
            vec![0x66, 0x6f, 0x6f],
            vec![0x66, 0x6f, 0x6f, 0x62],
            vec![0x66, 0x6f, 0x6f, 0x62, 0x61],
            vec![0x66, 0x6f, 0x6f, 0x62, 0x61, 0x72],
            vec![0xfb, 0xff, 0xfe, 0x00, 0x80, 0x7f, 0x01],
            b"MiraclePay v0.0.1".to_vec(),
        ];
        for case in cases {
            let encoded = base64url_encode(&case);
            assert!(!encoded.contains('='), "no padding: {encoded}");
            assert!(!encoded.contains('+'), "url-safe: {encoded}");
            assert!(!encoded.contains('/'), "url-safe: {encoded}");
            assert_eq!(base64url_decode(&encoded).unwrap(), case);
        }
        // Bytes that would be '+' and '/' in standard base64.
        assert_eq!(base64url_encode(&[0xfb, 0xff, 0xfe]), "-__-");
        assert_eq!(base64url_decode("-__-").unwrap(), vec![0xfb, 0xff, 0xfe]);
        assert!(base64url_decode("not*base64url").is_err());
        assert!(base64url_decode("a").is_err());
    }

    #[test]
    fn create_and_verify_roundtrip() {
        let token = create_token("usr_1", "merchant", SECRET);
        assert_eq!(token.split('.').count(), 3);
        let claims = verify_token(&token, SECRET).unwrap();
        assert_eq!(claims.sub, "usr_1");
        assert_eq!(claims.role, "merchant");
        assert_eq!(claims.merchant_id, None);
        let now = chrono::Utc::now().timestamp();
        assert!(claims.exp > now, "fresh token must not be expired");
        assert!(claims.exp <= now + TOKEN_TTL_SECS);
    }

    #[test]
    fn create_token_for_carries_merchant_scope() {
        let token = create_token_for("usr_2", "admin", Some("mch_9".to_string()), SECRET);
        let claims = verify_token(&token, SECRET).unwrap();
        assert_eq!(claims.sub, "usr_2");
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.merchant_id.as_deref(), Some("mch_9"));
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let token = create_token("usr_1", "merchant", SECRET);
        assert!(matches!(
            verify_token(&token, "a-different-secret"),
            Err(ApiError::Unauthorized)
        ));
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let token = create_token("usr_1", "merchant", SECRET);
        let parts: Vec<&str> = token.split('.').collect();
        // Swap in an "admin" payload but keep the original signature.
        let forged_payload = base64url_encode(
            serde_json::to_string(&Claims {
                sub: "usr_1".to_string(),
                role: "admin".to_string(),
                merchant_id: None,
                exp: chrono::Utc::now().timestamp() + TOKEN_TTL_SECS,
            })
            .unwrap()
            .as_bytes(),
        );
        let forged = format!("{}.{}.{}", parts[0], forged_payload, parts[2]);
        assert!(matches!(
            verify_token(&forged, SECRET),
            Err(ApiError::Unauthorized)
        ));
    }

    #[test]
    fn expired_token_is_rejected() {
        let expired = Claims {
            sub: "usr_1".to_string(),
            role: "merchant".to_string(),
            merchant_id: None,
            exp: chrono::Utc::now().timestamp() - 60,
        };
        let token = mint_token(&expired, SECRET);
        assert!(matches!(
            verify_token(&token, SECRET),
            Err(ApiError::Unauthorized)
        ));
    }

    #[test]
    fn malformed_tokens_are_rejected() {
        for token in ["", "abc", "a.b", "a.b.c.d", "..", "a.b.!"] {
            assert!(
                matches!(verify_token(token, SECRET), Err(ApiError::Unauthorized)),
                "token {token:?} must be rejected"
            );
        }
    }

    #[test]
    fn only_admin_role_is_admin() {
        let admin = AuthUser {
            user_id: "usr_1".to_string(),
            role: "admin".to_string(),
            merchant_id: None,
        };
        assert!(admin.is_admin());
        for role in ["merchant", "staff", "customer", "Admin", ""] {
            let user = AuthUser {
                user_id: "usr_2".to_string(),
                role: role.to_string(),
                merchant_id: None,
            };
            assert!(!user.is_admin(), "{role:?} must not be admin");
        }
    }
}
