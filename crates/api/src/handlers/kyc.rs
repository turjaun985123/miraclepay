//! Merchant KYC-lite endpoints (Wave 33).
//!
//! - `PUT /merchants/:id/kyc` — owner submits/updates the profile
//!   (validated by `miraclepay_domain::kyc`); status becomes
//!   `pending` and any previous decision is cleared.
//! - `GET /merchants/:id/kyc` — owner or admin reads the profile;
//!   a merchant without one gets `{status: "not_submitted"}`.
//! - `GET /admin/kyc/pending` — admin review queue.
//! - `POST /admin/merchants/:id/kyc/decision` — admin verifies or
//!   rejects (with an optional note).
//!
//! Enforcement: a merchant whose KYC is `rejected` cannot be set
//! Active (checked in `admin::set_merchant_status`).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::kyc::{validate_submission, KycProfile, KycStatus};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

/// Load a merchant the caller owns (owner-only actions).
async fn load_owned_merchant(
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

/// Body of `PUT /merchants/:id/kyc`.
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitKycReq {
    pub business_type: String,
    pub trade_license_no: Option<String>,
    pub nid_no: String,
    pub address: String,
}

/// `PUT /merchants/:id/kyc` — submit or resubmit the KYC profile.
pub async fn submit_kyc(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
    Json(req): Json<SubmitKycReq>,
) -> Result<(StatusCode, Json<KycProfile>), ApiError> {
    load_owned_merchant(&state, &auth, &merchant_id).await?;
    let errors = validate_submission(
        &req.business_type,
        req.trade_license_no.as_deref(),
        &req.nid_no,
        &req.address,
    );
    if !errors.is_empty() {
        return Err(ApiError::BadRequest(errors.join("; ")));
    }
    let profile = KycProfile {
        merchant_id: merchant_id.clone(),
        business_type: req.business_type.trim().to_string(),
        trade_license_no: req
            .trade_license_no
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        nid_no: req.nid_no.trim().to_string(),
        address: req.address.trim().to_string(),
        status: KycStatus::Pending,
        decision_note: None,
        submitted_at: Utc::now(),
        decided_at: None,
    };
    state
        .kyc_profiles
        .write()
        .await
        .insert(merchant_id.clone(), profile.clone());
    state.audit(
        Some(auth.user_id.clone()),
        Some(merchant_id),
        "merchant.kyc_submitted",
        "merchant",
        None,
    );
    Ok((StatusCode::OK, Json(profile)))
}

/// `GET /merchants/:id/kyc` — the profile, or a `not_submitted`
/// marker when the merchant has none yet.
pub async fn get_kyc(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    // Owner or admin may read.
    let merchants = state.merchants.read().await;
    let merchant = merchants
        .get(&merchant_id)
        .ok_or_else(|| ApiError::NotFound(format!("merchant {merchant_id} not found")))?;
    if merchant.owner_user_id != auth.user_id && !auth.is_admin() {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    drop(merchants);
    let profiles = state.kyc_profiles.read().await;
    match profiles.get(&merchant_id) {
        Some(profile) => Ok(Json(json!(profile))),
        None => Ok(Json(json!({
            "merchant_id": merchant_id,
            "status": "not_submitted",
        }))),
    }
}

/// `GET /admin/kyc/pending` — the admin review queue (oldest first).
pub async fn list_pending_kyc(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<KycProfile>>, ApiError> {
    if !auth.is_admin() {
        return Err(ApiError::Forbidden("admin role required".to_string()));
    }
    let profiles = state.kyc_profiles.read().await;
    let mut pending: Vec<KycProfile> = profiles
        .values()
        .filter(|profile| profile.status == KycStatus::Pending)
        .cloned()
        .collect();
    pending.sort_by_key(|profile| profile.submitted_at);
    Ok(Json(pending))
}

/// Body of `POST /admin/merchants/:id/kyc/decision`.
#[derive(Debug, Clone, Deserialize)]
pub struct KycDecisionReq {
    /// `"verified"` or `"rejected"`.
    pub decision: String,
    pub note: Option<String>,
}

/// `POST /admin/merchants/:id/kyc/decision` — verify or reject a
/// submitted profile.
pub async fn decide_kyc(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
    Json(req): Json<KycDecisionReq>,
) -> Result<Json<KycProfile>, ApiError> {
    if !auth.is_admin() {
        return Err(ApiError::Forbidden("admin role required".to_string()));
    }
    let new_status = match req.decision.as_str() {
        "verified" => KycStatus::Verified,
        "rejected" => KycStatus::Rejected,
        other => {
            return Err(ApiError::BadRequest(format!(
                "decision must be \"verified\" or \"rejected\", got \"{other}\""
            )))
        }
    };
    let mut profiles = state.kyc_profiles.write().await;
    let profile = profiles
        .get_mut(&merchant_id)
        .ok_or_else(|| ApiError::NotFound(format!("no KYC profile for merchant {merchant_id}")))?;
    if profile.status != KycStatus::Pending {
        return Err(ApiError::Conflict(format!(
            "KYC for merchant {merchant_id} is already {:?}",
            profile.status
        )));
    }
    profile.status = new_status;
    profile.decision_note = req.note;
    profile.decided_at = Some(Utc::now());
    let saved = profile.clone();
    drop(profiles);
    state.audit(
        Some(auth.user_id.clone()),
        Some(merchant_id),
        "merchant.kyc_decision",
        "merchant",
        None,
    );
    Ok(Json(saved))
}
