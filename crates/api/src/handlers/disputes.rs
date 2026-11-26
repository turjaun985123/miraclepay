//! Dispute / chargeback endpoints (Wave 38).
//!
//! - `POST /disputes` — the merchant (or an admin) opens a dispute
//!   record against one of the merchant's completed transactions.
//!   In production the trigger is the provider's chargeback
//!   notice; the record + workflow is what the platform owns.
//! - `GET /disputes`, `GET /disputes/:id` — owner-scoped reads
//!   (admins see all).
//! - `POST /disputes/:id/evidence` — merchant adds evidence while
//!   the dispute is open.
//! - `POST /admin/disputes/:id/decide` — admin records won/lost.
//!
//! Opening and deciding emit `dispute.opened` / `dispute.closed`
//! webhooks and audit entries. No money moves here (see the
//! domain module's honesty note).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::dispute::{validate_open, Dispute, DisputeReason, DisputeStatus};
use miraclepay_domain::ids::new_id;
use miraclepay_domain::money::{Currency, Money};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::pagination::{apply_page, PageQuery};
use crate::state::AppState;

/// Whether `auth` may see disputes of `merchant_id`.
async fn may_access(state: &AppState, auth: &AuthUser, merchant_id: &str) -> bool {
    if auth.is_admin() {
        return true;
    }
    let merchants = state.merchants.read().await;
    merchants
        .get(merchant_id)
        .map(|merchant| merchant.owner_user_id == auth.user_id)
        .unwrap_or(false)
}

/// Request body for [`open_dispute`].
#[derive(Debug, Clone, Deserialize)]
pub struct OpenDisputeRequest {
    pub transaction_id: String,
    pub amount_minor: i64,
    #[serde(default)]
    pub currency: Option<String>,
    pub reason: DisputeReason,
}

/// `POST /disputes` — open a dispute against a completed
/// transaction of a merchant the caller owns (admins: any).
pub async fn open_dispute(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<OpenDisputeRequest>,
) -> Result<(StatusCode, Json<Dispute>), ApiError> {
    let tx = {
        let transactions = state.transactions.read().await;
        transactions
            .get(&body.transaction_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::NotFound(format!("transaction {} not found", body.transaction_id))
            })?
    };
    if !may_access(&state, &auth, &tx.merchant_id).await {
        return Err(ApiError::NotFound(format!(
            "transaction {} not found",
            body.transaction_id
        )));
    }
    let currency = match &body.currency {
        Some(code) => Currency::from_code(&code.to_uppercase())
            .ok_or_else(|| ApiError::BadRequest(format!("unknown currency: {code}")))?,
        None => tx.amount.currency,
    };
    let amount = Money::new(body.amount_minor, currency);
    validate_open(&tx, amount).map_err(|errors| {
        ApiError::BadRequest(format!("cannot open dispute: {}", errors.join("; ")))
    })?;

    let mut disputes = state.disputes.write().await;
    let already_open = disputes
        .values()
        .any(|dispute| dispute.transaction_id == tx.id && !dispute.status.is_terminal());
    if already_open {
        return Err(ApiError::Conflict(format!(
            "transaction {} already has an open dispute",
            tx.id
        )));
    }
    let dispute = Dispute {
        id: new_id("dsp"),
        merchant_id: tx.merchant_id.clone(),
        transaction_id: tx.id.clone(),
        amount,
        reason: body.reason,
        status: DisputeStatus::Opened,
        evidence_note: None,
        outcome_note: None,
        opened_at: Utc::now(),
        decided_at: None,
    };
    disputes.insert(dispute.id.clone(), dispute.clone());
    drop(disputes);
    state.audit(
        Some(auth.user_id.clone()),
        Some(tx.merchant_id.clone()),
        "dispute.opened",
        "dispute",
        None,
    );
    state.emit_webhook(
        &tx.merchant_id,
        "dispute.opened",
        json!({
            "dispute_id": dispute.id,
            "transaction_id": dispute.transaction_id,
            "amount_minor": dispute.amount.amount_minor,
            "currency": dispute.amount.currency.code(),
            "reason": dispute.reason,
        }),
    );
    Ok((StatusCode::CREATED, Json(dispute)))
}

/// `GET /disputes` — disputes visible to the caller, newest first,
/// paginated.
pub async fn list_disputes(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(page): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let disputes = state.disputes.read().await;
    let mut visible: Vec<Dispute> = Vec::new();
    for dispute in disputes.values() {
        if may_access(&state, &auth, &dispute.merchant_id).await {
            visible.push(dispute.clone());
        }
    }
    visible.sort_by_key(|dispute| std::cmp::Reverse(dispute.opened_at));
    let total = visible.len();
    let items = apply_page(visible, page.page, page.per_page);
    Ok(Json(json!({ "items": items, "total": total })))
}

/// Load a dispute the caller may see.
async fn load_visible(state: &AppState, auth: &AuthUser, id: &str) -> Result<Dispute, ApiError> {
    let disputes = state.disputes.read().await;
    let dispute = disputes
        .get(id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("dispute {id} not found")))?;
    if may_access(state, auth, &dispute.merchant_id).await {
        Ok(dispute)
    } else {
        Err(ApiError::NotFound(format!("dispute {id} not found")))
    }
}

/// `GET /disputes/:id` — one dispute.
pub async fn get_dispute(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Dispute>, ApiError> {
    Ok(Json(load_visible(&state, &auth, &id).await?))
}

/// Request body for [`add_evidence`].
#[derive(Debug, Clone, Deserialize)]
pub struct EvidenceRequest {
    pub note: String,
}

/// `POST /disputes/:id/evidence` — merchant adds evidence while
/// the dispute is open; moves Opened → UnderReview.
pub async fn add_evidence(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EvidenceRequest>,
) -> Result<Json<Dispute>, ApiError> {
    if body.note.trim().is_empty() || body.note.chars().count() > 2_000 {
        return Err(ApiError::BadRequest(
            "evidence note must be 1..=2000 characters".to_string(),
        ));
    }
    load_visible(&state, &auth, &id).await?;
    let mut disputes = state.disputes.write().await;
    let dispute = disputes
        .get_mut(&id)
        .ok_or_else(|| ApiError::NotFound(format!("dispute {id} not found")))?;
    if dispute.status.is_terminal() {
        return Err(ApiError::Conflict(format!(
            "dispute {id} is already decided"
        )));
    }
    dispute.evidence_note = Some(body.note);
    if dispute.status == DisputeStatus::Opened {
        dispute.status = DisputeStatus::UnderReview;
    }
    Ok(Json(dispute.clone()))
}

/// Request body for [`decide_dispute`].
#[derive(Debug, Clone, Deserialize)]
pub struct DecideRequest {
    /// `won` (merchant keeps the funds) or `lost` (payer wins).
    pub outcome: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// `POST /admin/disputes/:id/decide` — admin records the outcome.
pub async fn decide_dispute(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<DecideRequest>,
) -> Result<Json<Dispute>, ApiError> {
    if !auth.is_admin() {
        return Err(ApiError::Forbidden("admin only".to_string()));
    }
    let status = match body.outcome.as_str() {
        "won" => DisputeStatus::Won,
        "lost" => DisputeStatus::Lost,
        other => {
            return Err(ApiError::BadRequest(format!(
                "outcome must be \"won\" or \"lost\", got {other:?}"
            )))
        }
    };
    let decided = {
        let mut disputes = state.disputes.write().await;
        let dispute = disputes
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("dispute {id} not found")))?;
        if dispute.status.is_terminal() {
            return Err(ApiError::Conflict(format!(
                "dispute {id} is already decided"
            )));
        }
        dispute.status = status;
        dispute.outcome_note = body.note;
        dispute.decided_at = Some(Utc::now());
        dispute.clone()
    };
    state.audit(
        Some(auth.user_id.clone()),
        Some(decided.merchant_id.clone()),
        "dispute.decided",
        "dispute",
        None,
    );
    state.emit_webhook(
        &decided.merchant_id,
        "dispute.closed",
        json!({
            "dispute_id": decided.id,
            "transaction_id": decided.transaction_id,
            "status": decided.status,
            "amount_minor": decided.amount.amount_minor,
            "currency": decided.amount.currency.code(),
        }),
    );
    Ok(Json(decided))
}
