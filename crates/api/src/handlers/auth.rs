//! Authentication handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! Endpoints:
//! - `POST /auth/register` — create a new merchant user account.
//! - `POST /auth/login` — exchange email + password for a token.
//! - `GET  /auth/me` (or `/me`) — return the authenticated user.
//!
//! Passwords are never stored or returned in plaintext, and the
//! stored `password_hash` is never included in any response.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::ids;
use miraclepay_domain::user::{validate_register, RegisterInput, User, UserRole};

use crate::auth_middleware::{create_token, AuthUser};
use crate::dto::ApiError;
use crate::state::AppState;

/// Request body for [`register`].
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterReq {
    pub name: String,
    pub email: String,
    pub password: String,
}

/// Request body for [`login`].
#[derive(Debug, Clone, Deserialize)]
pub struct LoginReq {
    pub email: String,
    pub password: String,
    /// TOTP code, required when the user has 2FA enabled (Wave 26).
    pub totp_code: Option<String>,
}

/// Snake-case string form of a [`UserRole`], matching its serde
/// representation (e.g. `UserRole::Merchant` -> `"merchant"`).
/// Used as the role claim when minting a token.
fn role_str(role: &UserRole) -> &'static str {
    match role {
        UserRole::Admin => "admin",
        UserRole::Merchant => "merchant",
        UserRole::Staff => "staff",
        UserRole::Customer => "customer",
    }
}

/// Public JSON view of a user. Built field by field so
/// `password_hash` can never leak into a response.
fn user_json(user: &User) -> Value {
    json!({
        "id": user.id,
        "name": user.name,
        "email": user.email,
        "role": user.role,
        "email_verified": user.email_verified,
        "totp_enabled": user.totp_enabled,
        "created_at": user.created_at,
    })
}

/// `POST /auth/register`
///
/// Validates the input with the domain rules, rejects a duplicate
/// email with `409 Conflict`, creates the user with the
/// [`UserRole::Merchant`] role, stores it, and returns `201` with
/// `{id, email, role}`.
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let name = req.name.trim().to_string();
    // Normalise the email once so duplicate checks, storage, and
    // the password hash all agree regardless of input casing.
    let email = req.email.trim().to_lowercase();

    let input = RegisterInput {
        name: name.clone(),
        email: email.clone(),
        password: req.password.clone(),
    };
    if let Err(errors) = validate_register(&input) {
        return Err(ApiError::BadRequest(errors.join("; ")));
    }

    // Wave 3: Argon2id with a per-password CSPRNG salt.
    let password_hash = miraclepay_domain::password::hash_password(&req.password);

    let user = User {
        id: ids::user_id(),
        name,
        email: email.clone(),
        password_hash,
        role: UserRole::Merchant,
        email_verified: false,
        totp_secret: None,
        totp_enabled: false,
        created_at: chrono::Utc::now(),
    };

    {
        let mut users = state.users.write().await;
        if users
            .values()
            .any(|existing| existing.email.eq_ignore_ascii_case(&email))
        {
            return Err(ApiError::Conflict("email already registered".to_string()));
        }
        users.insert(user.id.clone(), user.clone());
    }

    state.audit(
        Some(user.id.clone()),
        None,
        "auth.register",
        "user",
        Some(user.id.clone()),
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": user.id,
            "email": user.email,
            "role": user.role,
        })),
    ))
}

/// `POST /auth/login`
///
/// Finds the user by email, compares the password hash, and on
/// success returns `{token, user: {id, email, role}}`. Any failure
/// — unknown email or wrong password — returns `401 Unauthorized`
/// without revealing which part failed.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<Value>, ApiError> {
    let email = req.email.trim().to_lowercase();

    let user = {
        let users = state.users.read().await;
        users
            .values()
            .find(|existing| existing.email.eq_ignore_ascii_case(&email))
            .cloned()
    };

    let user = match user {
        Some(user) => user,
        None => return Err(ApiError::Unauthorized),
    };

    // Wave 3: verify against the stored Argon2id PHC hash (the
    // candidate computation above); no email is mixed into the hash.
    let candidate_ok =
        miraclepay_domain::password::verify_password(&req.password, &user.password_hash);
    if !candidate_ok {
        return Err(ApiError::Unauthorized);
    }

    // Wave 26: TOTP second factor, when the user enabled it.
    if user.totp_enabled {
        let secret = user.totp_secret.as_deref().unwrap_or("");
        match req.totp_code.as_deref() {
            None => return Err(ApiError::TotpRequired),
            Some(code) => {
                let now = chrono::Utc::now().timestamp().max(0) as u64;
                if !miraclepay_domain::totp::verify(secret, code, now) {
                    return Err(ApiError::TotpInvalid);
                }
            }
        }
    }

    let token = create_token(&user.id, role_str(&user.role), &state.jwt_secret);
    // Wave 25: a rotating refresh token alongside the 24h JWT.
    let refresh_token = crate::refresh::issue_refresh_token(&state, &user.id).await;

    state.audit(
        Some(user.id.clone()),
        None,
        "auth.login",
        "user",
        Some(user.id.clone()),
    );

    Ok(Json(json!({
        "token": token,
        "refresh_token": refresh_token,
        "user": {
            "id": user.id,
            "email": user.email,
            "role": user.role,
        },
    })))
}

/// Body of `POST /auth/refresh` and `POST /auth/logout`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RefreshReq {
    pub refresh_token: String,
}

/// `POST /auth/refresh` (Wave 25) — exchange a live refresh token
/// for a fresh `{token, refresh_token}` pair. The presented token
/// is revoked by the rotation (see `crate::refresh`), so a stolen
/// old token cannot be replayed after a legitimate refresh.
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshReq>,
) -> Result<Json<Value>, ApiError> {
    let user_id = crate::refresh::rotate_refresh_token(&state, &req.refresh_token)
        .await
        .map_err(|_| ApiError::Unauthorized)?;

    let user = {
        let users = state.users.read().await;
        users.get(&user_id).cloned()
    };
    let user = match user {
        Some(user) => user,
        None => return Err(ApiError::Unauthorized),
    };

    let token = create_token(&user.id, role_str(&user.role), &state.jwt_secret);
    let refresh_token = crate::refresh::issue_refresh_token(&state, &user.id).await;

    state.audit(
        Some(user.id.clone()),
        None,
        "auth.refresh",
        "user",
        Some(user.id.clone()),
    );

    Ok(Json(json!({
        "token": token,
        "refresh_token": refresh_token,
        "user": {
            "id": user.id,
            "email": user.email,
            "role": user.role,
        },
    })))
}

/// `POST /auth/logout` (Wave 25) — revoke the presented refresh
/// token. Idempotent: unknown or already-revoked tokens still
/// return 200 with `revoked: false`, so clients can call it
/// unconditionally at sign-out.
pub async fn logout(
    State(state): State<AppState>,
    Json(req): Json<RefreshReq>,
) -> Result<Json<Value>, ApiError> {
    let revoked = crate::refresh::revoke_refresh_token(&state, &req.refresh_token).await;
    Ok(Json(json!({ "revoked": revoked })))
}

/// `POST /auth/2fa/setup` (Wave 26) — generate a TOTP secret for
/// the signed-in user and return it (once) with the otpauth URI.
/// The secret is stored as *pending*: login does not require a
/// code until `POST /auth/2fa/enable` confirms possession.
pub async fn totp_setup(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let secret = miraclepay_domain::totp::generate_secret();
    let mut users = state.users.write().await;
    let user = users.get_mut(&auth.user_id).ok_or(ApiError::Unauthorized)?;
    user.totp_secret = Some(secret.clone());
    let email = user.email.clone();
    drop(users);
    state.audit(
        Some(auth.user_id.clone()),
        None,
        "auth.2fa.setup",
        "user",
        Some(auth.user_id.clone()),
    );
    Ok(Json(json!({
        "secret": secret,
        "otpauth_uri": miraclepay_domain::totp::otpauth_uri(&secret, &email, "MiraclePay"),
        "enabled": false,
    })))
}

/// Body of the 2FA enable/disable endpoints.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TotpCodeReq {
    pub code: String,
}

/// `POST /auth/2fa/enable` (Wave 26) — confirm a code from the
/// authenticator app against the pending secret, then enforce
/// TOTP at login.
pub async fn totp_enable(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<TotpCodeReq>,
) -> Result<Json<Value>, ApiError> {
    let mut users = state.users.write().await;
    let user = users.get_mut(&auth.user_id).ok_or(ApiError::Unauthorized)?;
    let secret = user
        .totp_secret
        .clone()
        .ok_or_else(|| ApiError::BadRequest("2fa setup has not been started".to_string()))?;
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if !miraclepay_domain::totp::verify(&secret, &req.code, now) {
        return Err(ApiError::TotpInvalid);
    }
    user.totp_enabled = true;
    drop(users);
    state.audit(
        Some(auth.user_id.clone()),
        None,
        "auth.2fa.enable",
        "user",
        Some(auth.user_id.clone()),
    );
    Ok(Json(json!({ "enabled": true })))
}

/// `POST /auth/2fa/disable` (Wave 26) — turn 2FA off after proving
/// possession with a current code; clears the stored secret.
pub async fn totp_disable(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<TotpCodeReq>,
) -> Result<Json<Value>, ApiError> {
    let mut users = state.users.write().await;
    let user = users.get_mut(&auth.user_id).ok_or(ApiError::Unauthorized)?;
    if !user.totp_enabled {
        return Err(ApiError::BadRequest("2fa is not enabled".to_string()));
    }
    let secret = user.totp_secret.clone().unwrap_or_default();
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if !miraclepay_domain::totp::verify(&secret, &req.code, now) {
        return Err(ApiError::TotpInvalid);
    }
    user.totp_enabled = false;
    user.totp_secret = None;
    drop(users);
    state.audit(
        Some(auth.user_id.clone()),
        None,
        "auth.2fa.disable",
        "user",
        Some(auth.user_id.clone()),
    );
    Ok(Json(json!({ "enabled": false })))
}

/// `GET /auth/me`
///
/// Returns the currently authenticated user as JSON. The response
/// is built via [`user_json`], so `password_hash` is never exposed.
pub async fn me(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let user = {
        let users = state.users.read().await;
        users
            .get(&auth.user_id)
            .cloned()
            .or_else(|| users.values().find(|u| u.id == auth.user_id).cloned())
    };

    let user = match user {
        Some(user) => user,
        None => {
            return Err(ApiError::NotFound(format!(
                "user {} not found",
                auth.user_id
            )))
        }
    };

    Ok(Json(user_json(&user)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn different_passwords_do_not_cross_verify() {
        // Wave 3 semantics: Argon2id verification, not hash equality.
        let hash = miraclepay_domain::password::hash_password("password1");
        assert!(miraclepay_domain::password::verify_password(
            "password1",
            &hash
        ));
        assert!(!miraclepay_domain::password::verify_password(
            "password2",
            &hash
        ));
    }

    #[test]
    fn same_password_verifies_across_distinct_salted_hashes() {
        let first = miraclepay_domain::password::hash_password("password1");
        let second = miraclepay_domain::password::hash_password("password1");
        assert_ne!(first, second); // random salt per hash
        assert!(miraclepay_domain::password::verify_password(
            "password1",
            &first
        ));
        assert!(miraclepay_domain::password::verify_password(
            "password1",
            &second
        ));
    }

    #[test]
    fn user_json_never_contains_password_hash() {
        let user = User {
            id: "usr_1".to_string(),
            name: "Ayesha Rahman".to_string(),
            email: "ayesha@example.com".to_string(),
            password_hash: "stored-hash-value".to_string(),
            role: UserRole::Merchant,
            email_verified: false,
            totp_secret: None,
            totp_enabled: false,
            created_at: chrono::Utc::now(),
        };
        let value = user_json(&user);
        let rendered = value.to_string();
        assert!(!rendered.contains("password_hash"), "{rendered}");
        assert!(!rendered.contains("stored-hash-value"), "{rendered}");
        assert_eq!(value["email"], json!("ayesha@example.com"));
        assert_eq!(value["role"], json!("merchant"));
    }
}
