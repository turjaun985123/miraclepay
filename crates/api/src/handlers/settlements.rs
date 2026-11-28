//! Settlement endpoints (Wave 18) — the flow made end-to-end.
//!
//! Settlements previously existed only as a worker job
//! (`miraclepay_worker::settlement_job`) and a balance input: no API
//! could list, create, or pay one. These handlers wire the job into
//! the API:
//!
//! - `GET /settlements` — owner-scoped list (+ filters, paging).
//! - `POST /settlements/run` — build settlements for one merchant
//!   and period from its completed transactions, using the worker's
//!   `collect_candidates` + `build_settlements`. Transactions whose
//!   creation time falls inside an earlier settlement's period for
//!   the same merchant are treated as already settled and excluded,
//!   so a period is never paid twice.
//! - `POST /settlements/:id/pay` — record the manual payout via the
//!   worker's `mark_paid` (stores the payout reference).

use std::collections::HashSet;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::money::Money;
use miraclepay_domain::settlement::{Settlement, SettlementStatus};
use serde::Serialize;

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

/// Serialised (snake_case) form of a settlement status, used for
/// the `status` query filter. (Previously the filter compared the
/// `Debug` spelling, so `?status=on_hold` — the serialised form —
/// never matched an `OnHold` settlement.)
fn status_label(status: SettlementStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// A settlement plus the display fields a merchant payouts table
/// needs (Stream C, v0.0.4): the derived net payout and the owning
/// merchant's business name. All pre-existing `Settlement` fields
/// (`id`, `merchant_id`, `gross`, `fee`, `status`, `period_start`,
/// `period_end`, `payout_method`, `payout_reference`, `created_at`)
/// are preserved verbatim via flattening.
#[derive(Debug, Clone, Serialize)]
pub struct SettlementView {
    #[serde(flatten)]
    pub settlement: Settlement,
    /// Net payout (`gross - fee`) as a `Money` object; `null` when
    /// gross/fee currencies mismatch (net is uncomputable).
    pub net: Option<Money>,
    /// Net payout in minor units (same value as `net.amount_minor`).
    pub net_minor: Option<i64>,
    /// Business name of `merchant_id`, when the merchant exists.
    pub merchant_name: Option<String>,
}

/// Build a [`SettlementView`] from a stored settlement.
async fn settlement_view(state: &AppState, settlement: Settlement) -> SettlementView {
    let merchant_name = {
        let merchants = state.merchants.read().await;
        merchants
            .get(&settlement.merchant_id)
            .map(|m| m.business_name.clone())
    };
    let net = settlement.net().ok();
    SettlementView {
        net_minor: net.map(|net| net.amount_minor),
        net,
        merchant_name,
        settlement,
    }
}

/// Whether the caller may act on `merchant_id` (owner or admin).
async fn may_access(state: &AppState, auth: &AuthUser, merchant_id: &str) -> bool {
    if auth.is_admin() {
        return true;
    }
    let merchants = state.merchants.read().await;
    merchants
        .get(merchant_id)
        .map(|m| m.owner_user_id == auth.user_id)
        .unwrap_or(false)
}

/// Query for `GET /settlements`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListSettlementsQuery {
    pub merchant_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// `GET /settlements` — settlements of merchants the caller owns
/// (admins see all), newest period first. Response shape is
/// unchanged (a bare JSON array, paged via `pagination::apply_page`
/// when `page`/`per_page` are supplied); each item is now a
/// [`SettlementView`] — every `Settlement` field plus `net`,
/// `net_minor` and `merchant_name`. The `status` filter takes the
/// serialised snake_case status (e.g. `"on_hold"`, `"paid"`).
pub async fn list_settlements(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListSettlementsQuery>,
) -> Result<Json<Vec<SettlementView>>, ApiError> {
    let settlements = state.settlements.read().await;
    let mut items: Vec<Settlement> = Vec::new();
    for settlement in settlements.values() {
        if let Some(mid) = query.merchant_id.as_deref() {
            if settlement.merchant_id != mid {
                continue;
            }
        }
        if let Some(status) = query.status.as_deref() {
            if status_label(settlement.status) != status.trim().to_ascii_lowercase() {
                continue;
            }
        }
        items.push(settlement.clone());
    }
    drop(settlements);

    // Ownership filter needs merchant lookups, so it runs after the
    // cheap field filters.
    let mut owned: Vec<Settlement> = Vec::new();
    for settlement in items {
        if may_access(&state, &auth, &settlement.merchant_id).await {
            owned.push(settlement);
        }
    }
    owned.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    let owned = crate::pagination::apply_page(owned, query.page, query.per_page);
    let mut views: Vec<SettlementView> = Vec::with_capacity(owned.len());
    for settlement in owned {
        views.push(settlement_view(&state, settlement).await);
    }
    Ok(Json(views))
}

/// `GET /settlements/:id` — one settlement as a [`SettlementView`]
/// (Stream C, v0.0.4; previously there was no detail endpoint, so a
/// payouts UI could not render a detail view). Owner-scoped like
/// the dispute detail: a settlement the caller may not see answers
/// `404`, not `403`, so existence is not leaked.
pub async fn get_settlement(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SettlementView>, ApiError> {
    let settlement = {
        let settlements = state.settlements.read().await;
        settlements
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("settlement {id} not found")))?
    };
    if !may_access(&state, &auth, &settlement.merchant_id).await {
        return Err(ApiError::NotFound(format!("settlement {id} not found")));
    }
    Ok(Json(settlement_view(&state, settlement).await))
}

/// Body of `POST /settlements/run`.
#[derive(Debug, Clone, Deserialize)]
pub struct RunSettlementsReq {
    pub merchant_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    /// Fee in basis points; defaults to the platform setting.
    /// Admin-only: a non-admin supplying it gets `403` (S4).
    pub fee_bps: Option<u32>,
}

/// `POST /settlements/run` — create settlements for one merchant's
/// completed transactions in `[period_start, period_end)`.
pub async fn run_settlements(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<RunSettlementsReq>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if req.period_end <= req.period_start {
        return Err(ApiError::BadRequest(
            "period_end must be after period_start".to_string(),
        ));
    }
    if !may_access(&state, &auth, &req.merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    // S4 (Stream C security): a non-admin caller must not be able
    // to set the fee — a merchant passing `fee_bps: 0` would waive
    // their own platform fee. Reject it outright; omitting it
    // resolves the merchant's effective fee (their `fee_bps`
    // override when set, else the platform default). Admins may
    // still set it explicitly.
    if !auth.is_admin() && req.fee_bps.is_some() {
        return Err(ApiError::Forbidden(
            "fee_bps may only be set by an admin; omit it to use your configured fee".to_string(),
        ));
    }
    let fee_bps = match req.fee_bps {
        Some(bps) => bps,
        None => {
            let platform = state.settings.read().await;
            let all = state.merchant_settings.read().await;
            match all.get(&req.merchant_id) {
                Some(ms) => ms.effective_fee_bps(&platform),
                None => platform.default_fee_bps,
            }
        }
    };

    // Transactions already covered by an earlier settlement period
    // for this merchant count as settled (never pay a period twice).
    let (transactions, already_settled) = {
        let settlements = state.settlements.read().await;
        let periods: Vec<(DateTime<Utc>, DateTime<Utc>)> = settlements
            .values()
            .filter(|s| s.merchant_id == req.merchant_id && s.status != SettlementStatus::Failed)
            .map(|s| (s.period_start, s.period_end))
            .collect();
        let transactions = state.transactions.read().await;
        let already: HashSet<String> = transactions
            .values()
            .filter(|tx| {
                periods
                    .iter()
                    .any(|(start, end)| tx.created_at >= *start && tx.created_at < *end)
            })
            .map(|tx| tx.id.clone())
            .collect();
        let txs: Vec<_> = transactions
            .values()
            .filter(|tx| tx.merchant_id == req.merchant_id)
            .cloned()
            .collect();
        (txs, already)
    };

    let candidates = miraclepay_worker::settlement_job::collect_candidates(
        &transactions,
        &already_settled,
        req.period_start,
        req.period_end,
    );
    let run_prefix = miraclepay_domain::ids::settlement_id();
    let built = miraclepay_worker::settlement_job::build_settlements(
        &candidates,
        fee_bps,
        req.period_start,
        req.period_end,
        &run_prefix,
    );

    {
        let mut settlements = state.settlements.write().await;
        for settlement in &built {
            settlements.insert(settlement.id.clone(), settlement.clone());
        }
    }
    state.audit(
        Some(auth.user_id.clone()),
        Some(req.merchant_id.clone()),
        "settlement.run",
        "settlement",
        None,
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "created": built.len(),
            "settlements": built,
        })),
    ))
}

/// Body of `POST /settlements/:id/pay`.
#[derive(Debug, Clone, Deserialize)]
pub struct PaySettlementReq {
    /// Reference from the payout rail (bank/wallet trx id).
    pub payout_reference: String,
}

/// `POST /settlements/:id/pay` — record that a settlement's manual
/// payout was sent, storing the payout reference.
///
/// S5 (Stream C security): **admin-only**, mirroring
/// `disputes::decide_dispute`. Previously the merchant owner could
/// mark their own settlement paid; because paid settlements' nets
/// are subtracted in balance computation, a self-reported "paid"
/// corrupted the merchant's available balance.
pub async fn pay_settlement(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PaySettlementReq>,
) -> Result<Json<Settlement>, ApiError> {
    if !auth.is_admin() {
        return Err(ApiError::Forbidden("admin only".to_string()));
    }
    if req.payout_reference.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "payout_reference must not be empty".to_string(),
        ));
    }
    let mut settlements = state.settlements.write().await;
    let settlement = settlements
        .get_mut(&id)
        .ok_or_else(|| ApiError::NotFound(format!("settlement {id} not found")))?;
    if !may_access(&state, &auth, &settlement.merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    miraclepay_worker::settlement_job::mark_paid(settlement, req.payout_reference.trim())
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    let saved = settlement.clone();
    drop(settlements);
    state.audit(
        Some(auth.user_id.clone()),
        Some(saved.merchant_id.clone()),
        "settlement.paid",
        "settlement",
        Some(saved.id.clone()),
    );
    // Wave 32: webhook event alongside the audit entry.
    state.emit_webhook(
        &saved.merchant_id,
        "settlement.paid",
        json!({
            "settlement_id": saved.id,
            "merchant_id": saved.merchant_id,
            "gross_minor": saved.gross.amount_minor,
            "fee_minor": saved.fee.amount_minor,
            "net_minor": saved.net().map(|net| net.amount_minor).unwrap_or(0),
            "currency": saved.gross.currency.code(),
            "payout_reference": saved.payout_reference,
        }),
    );
    Ok(Json(saved))
}
