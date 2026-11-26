//! Password reset, email verification and session management
//! (Waves 88–90).
//!
//! - `POST /auth/password-reset/request` — always 200 (no account
//!   enumeration); when the email exists, a 1-hour token is issued
//!   and emailed through the notification queue.
//! - `POST /auth/password-reset/confirm` — spends the token, sets
//!   the new Argon2id password, and revokes every refresh token
//!   (all sessions) for the account.
//! - `POST /auth/email-verification/request` (auth) — 24-hour
//!   token emailed to the account address.
//! - `POST /auth/email-verification/confirm` — spends the token,
//!   marks the email verified.
//! - `GET /auth/sessions` (auth) — the caller's refresh-token
//!   sessions (id = token hash; the raw token is never stored or
//!   returned).
//! - `DELETE /auth/sessions/:id` (auth) — revoke one session.

use axum::extract::{Path, State};
use axum::Json;
use chrono::Duration;
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::notification::{Notification, NotificationChannel};

use crate::auth_middleware::AuthUser;
use crate::auth_tokens::{self, PURPOSE_EMAIL_VERIFY, PURPOSE_PASSWORD_RESET};
use crate::dto::ApiError;
use crate::state::AppState;

async fn queue_email(state: &AppState, to: &str, subject: &str, body: String) {
    let notification = Notification::new(
        miraclepay_domain::ids::new_id("ntf"),
        None,
        NotificationChannel::Email,
        to.to_string(),
        Some(subject.to_string()),
        body,
    );
    state.notifications.write().await.push(notification);
}

#[derive(Debug, Deserialize)]
pub struct EmailReq {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetConfirmReq {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct TokenReq {
    pub token: String,
}

fn validate_new_password(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < 8 || !password.chars().any(|c| c.is_ascii_digit()) {
        return Err(ApiError::BadRequest(
            "password must be at least 8 characters and contain a digit".to_string(),
        ));
    }
    Ok(())
}

/// `POST /auth/password-reset/request` (Wave 88).
pub async fn password_reset_request(
    State(state): State<AppState>,
    Json(req): Json<EmailReq>,
) -> Result<Json<Value>, ApiError> {
    let email = req.email.trim().to_lowercase();
    let user = state
        .users
        .read()
        .await
        .values()
        .find(|u| u.email.eq_ignore_ascii_case(&email))
        .cloned();
    if let Some(user) = user {
        let token =
            auth_tokens::issue(&state, PURPOSE_PASSWORD_RESET, &user.id, Duration::hours(1)).await;
        queue_email(
            &state,
            &user.email,
            "Reset your MiraclePay password",
            format!(
                "Use this token within 1 hour to reset your password:\n\n{token}\n\n\
                 If you did not request this, ignore this email."
            ),
        )
        .await;
        state.audit(
            Some(user.id.clone()),
            None,
            "auth.password_reset_requested",
            "user",
            Some(user.id.clone()),
        );
    }
    // Identical answer whether or not the account exists.
    Ok(Json(json!({"ok": true})))
}

/// `POST /auth/password-reset/confirm` (Wave 88).
pub async fn password_reset_confirm(
    State(state): State<AppState>,
    Json(req): Json<ResetConfirmReq>,
) -> Result<Json<Value>, ApiError> {
    validate_new_password(&req.new_password)?;
    let user_id = auth_tokens::consume(&state, PURPOSE_PASSWORD_RESET, &req.token)
        .await
        .ok_or(ApiError::Unauthorized)?;
    {
        let mut users = state.users.write().await;
        let user = users
            .get_mut(&user_id)
            .ok_or_else(|| ApiError::NotFound("user".to_string()))?;
        user.password_hash = miraclepay_domain::password::hash_password(&req.new_password);
    }
    // A password reset ends every existing session.
    {
        let mut tokens = state.refresh_tokens.write().await;
        for record in tokens.values_mut() {
            if record.user_id == user_id {
                record.revoked = true;
            }
        }
    }
    state.audit(
        Some(user_id.clone()),
        None,
        "auth.password_reset_completed",
        "user",
        Some(user_id),
    );
    Ok(Json(json!({"ok": true})))
}

/// `POST /auth/email-verification/request` (Wave 89).
pub async fn email_verification_request(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .users
        .read()
        .await
        .get(&auth.user_id)
        .cloned()
        .ok_or(ApiError::Unauthorized)?;
    if user.email_verified {
        return Ok(Json(json!({"ok": true, "already_verified": true})));
    }
    let token =
        auth_tokens::issue(&state, PURPOSE_EMAIL_VERIFY, &user.id, Duration::hours(24)).await;
    queue_email(
        &state,
        &user.email,
        "Verify your MiraclePay email",
        format!("Use this token within 24 hours to verify your email address:\n\n{token}"),
    )
    .await;
    Ok(Json(json!({"ok": true})))
}

/// `POST /auth/email-verification/confirm` (Wave 89).
pub async fn email_verification_confirm(
    State(state): State<AppState>,
    Json(req): Json<TokenReq>,
) -> Result<Json<Value>, ApiError> {
    let user_id = auth_tokens::consume(&state, PURPOSE_EMAIL_VERIFY, &req.token)
        .await
        .ok_or(ApiError::Unauthorized)?;
    let mut users = state.users.write().await;
    let user = users
        .get_mut(&user_id)
        .ok_or_else(|| ApiError::NotFound("user".to_string()))?;
    user.email_verified = true;
    Ok(Json(json!({"ok": true, "email_verified": true})))
}

/// `GET /auth/sessions` (Wave 90).
pub async fn list_sessions(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let tokens = state.refresh_tokens.read().await;
    let sessions: Vec<Value> = tokens
        .iter()
        .filter(|(_, record)| record.user_id == auth.user_id)
        .map(|(hash, record)| {
            json!({
                "id": hash,
                "expires_at": record.expires_at,
                "revoked": record.revoked,
                "active": !record.revoked && record.expires_at > chrono::Utc::now(),
            })
        })
        .collect();
    Ok(Json(json!({"sessions": sessions})))
}

/// `DELETE /auth/sessions/:id` (Wave 90).
pub async fn revoke_session(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let mut tokens = state.refresh_tokens.write().await;
    let record = tokens
        .get_mut(&id)
        .filter(|record| record.user_id == auth.user_id)
        .ok_or_else(|| ApiError::NotFound("session".to_string()))?;
    record.revoked = true;
    Ok(Json(json!({"ok": true, "revoked": true})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_password_rules() {
        assert!(validate_new_password("short1").is_err());
        assert!(validate_new_password("longenoughnodigits").is_err());
        assert!(validate_new_password("longenough1").is_ok());
    }
}
