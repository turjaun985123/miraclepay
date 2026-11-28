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
use serde::Serialize;

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::pagination::{apply_page, PageQuery};
use crate::state::AppState;

/// Serialised (snake_case) form of a dispute status, used for the
/// `status` query filter (same approach as `transactions`).
fn status_label(status: DisputeStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// A dispute plus the display fields a merchant dashboard table /
/// detail view needs (Stream C, v0.0.4): the owning merchant's
/// business name and the challenged transaction's reference,
/// provider id, gateway and customer contact. All pre-existing
/// `Dispute` fields are preserved verbatim via flattening —
/// including `evidence_note` and `evidence_files`, so the detail
/// response carries its evidence entries.
#[derive(Debug, Clone, Serialize)]
pub struct DisputeView {
    #[serde(flatten)]
    pub dispute: Dispute,
    /// Business name of `merchant_id`, when the merchant exists.
    pub merchant_name: Option<String>,
    /// Merchant/customer reference of the challenged transaction.
    pub transaction_reference: Option<String>,
    /// Provider-assigned transaction id of the challenged payment.
    pub provider_trx_id: Option<String>,
    /// Gateway that processed the challenged payment.
    pub gateway: Option<String>,
    /// Customer email on the challenged transaction, when known.
    pub customer_email: Option<String>,
    /// Customer phone on the challenged transaction, when known.
    pub customer_phone: Option<String>,
}

/// Build a [`DisputeView`] by joining the transaction + merchant
/// display fields onto `dispute` (missing joins yield `null`s).
async fn dispute_view(state: &AppState, dispute: Dispute) -> DisputeView {
    let merchant_name = {
        let merchants = state.merchants.read().await;
        merchants
            .get(&dispute.merchant_id)
            .map(|merchant| merchant.business_name.clone())
    };
    let tx = {
        let transactions = state.transactions.read().await;
        transactions.get(&dispute.transaction_id).cloned()
    };
    DisputeView {
        transaction_reference: tx.as_ref().and_then(|tx| tx.reference.clone()),
        provider_trx_id: tx.as_ref().and_then(|tx| tx.provider_trx_id.clone()),
        gateway: tx.as_ref().map(|tx| tx.gateway.clone()),
        customer_email: tx.as_ref().and_then(|tx| tx.customer_email.clone()),
        customer_phone: tx.as_ref().and_then(|tx| tx.customer_phone.clone()),
        merchant_name,
        dispute,
    }
}

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
        evidence_files: Vec::new(),
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

/// Query for `GET /disputes` (Stream C, v0.0.4): paging plus the
/// filters a dashboard table needs. `merchant_id` can only narrow
/// the caller's own scope — owner-scoping is applied first, so
/// naming another merchant yields an empty list, never their data.
/// `status` is the serialised snake_case status (e.g.
/// `"under_review"`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListDisputesQuery {
    pub merchant_id: Option<String>,
    pub status: Option<String>,
    /// Exact dispute reason label (e.g. `"fraudulent"`),
    /// case-insensitive.
    pub reason: Option<String>,
    /// Only disputes opened against this transaction.
    pub transaction_id: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// `GET /disputes` — disputes visible to the caller, newest first,
/// paginated. Response: `{ items: [DisputeView], total, page,
/// per_page, total_pages }` (`items`/`total` are the pre-existing
/// fields; the page metadata is additive).
pub async fn list_disputes(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListDisputesQuery>,
) -> Result<Json<Value>, ApiError> {
    let disputes = state.disputes.read().await;
    let mut visible: Vec<Dispute> = Vec::new();
    for dispute in disputes.values() {
        if may_access(&state, &auth, &dispute.merchant_id).await {
            visible.push(dispute.clone());
        }
    }
    drop(disputes);
    if let Some(mid) = query.merchant_id.as_deref() {
        visible.retain(|dispute| dispute.merchant_id == mid);
    }
    if let Some(status) = query.status.as_deref() {
        let wanted = status.trim().to_ascii_lowercase();
        visible.retain(|dispute| status_label(dispute.status) == wanted);
    }
    if let Some(reason) = query.reason.as_deref() {
        let wanted = reason.trim().to_ascii_lowercase();
        visible.retain(|dispute| {
            serde_json::to_value(dispute.reason)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default()
                == wanted
        });
    }
    if let Some(tid) = query.transaction_id.as_deref() {
        visible.retain(|dispute| dispute.transaction_id == tid);
    }
    visible.sort_by_key(|dispute| std::cmp::Reverse(dispute.opened_at));
    let total = visible.len();
    let page_query = PageQuery::new(query.page, query.per_page);
    let paged = apply_page(visible, query.page, query.per_page);
    let mut items: Vec<DisputeView> = Vec::with_capacity(paged.len());
    for dispute in paged {
        items.push(dispute_view(&state, dispute).await);
    }
    let meta = crate::pagination::PageMeta::new(total as u64, &page_query);
    Ok(Json(json!({
        "items": items,
        "total": total,
        "page": meta.page,
        "per_page": meta.per_page,
        "total_pages": meta.total_pages,
    })))
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

/// `GET /disputes/:id` — one dispute as a [`DisputeView`]: all
/// `Dispute` fields (including `evidence_note` + `evidence_files`)
/// plus the joined transaction/merchant display fields.
pub async fn get_dispute(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DisputeView>, ApiError> {
    let dispute = load_visible(&state, &auth, &id).await?;
    Ok(Json(dispute_view(&state, dispute).await))
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

/// Request body for [`add_evidence_file`] (Wave 100).
#[derive(Debug, Clone, Deserialize)]
pub struct EvidenceFileRequest {
    pub name: String,
    pub url: String,
}

/// `POST /disputes/:id/evidence/files` — attach an evidence file
/// reference (HTTPS URL to a hosted file; the platform stores the
/// reference, not the bytes) while the dispute is undecided.
pub async fn add_evidence_file(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EvidenceFileRequest>,
) -> Result<Json<Dispute>, ApiError> {
    if body.name.trim().is_empty() || body.name.chars().count() > 200 {
        return Err(ApiError::BadRequest(
            "evidence file name must be 1..=200 characters".to_string(),
        ));
    }
    if !body.url.starts_with("https://") || body.url.len() > 2_000 {
        return Err(ApiError::BadRequest(
            "evidence file url must be an https URL of at most 2000 characters".to_string(),
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
    dispute
        .evidence_files
        .push(miraclepay_domain::dispute::EvidenceFile {
            name: body.name,
            url: body.url,
            added_at: Utc::now(),
        });
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
