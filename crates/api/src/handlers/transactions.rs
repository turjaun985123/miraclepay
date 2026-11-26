//! Transaction and refund handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation. Axum 0.7.
//!
//! Endpoints:
//!
//! * `list_transactions` — list transactions, newest first. A
//!   non-admin caller only ever sees transactions belonging to their
//!   own merchant; an admin sees every merchant's transactions.
//! * `get_transaction` — fetch one transaction by id (owner or admin).
//! * `create_refund` — refund (part of) a completed transaction,
//!   through the same gateway that processed the payment when that
//!   gateway supports refunds, and as a locally recorded, manually
//!   processed refund when it does not.
//! * `list_refunds` — list refunds, owner-scoped like the
//!   transaction list.
//!
//! All state is held in [`crate::state::AppState`]; every handler is
//! a thin, individually callable unit over the shared domain types.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::errors::DomainError;
use miraclepay_domain::ids::{refund_id, webhook_event_id};
use miraclepay_domain::money::Money;
use miraclepay_domain::refund::{validate_refund, Refund, RefundStatus};
use miraclepay_domain::transaction::{Transaction, TxStatus};
use miraclepay_domain::webhook::WebhookEvent;
use miraclepay_gateways::types::{GatewayError, RefundRequest};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Query / request bodies
// ---------------------------------------------------------------------------

/// Query parameters for `list_transactions`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListQ {
    /// Restrict to one merchant. Honoured for admins; for non-admin
    /// callers it can only narrow their own merchant's transactions,
    /// never widen the scope to another merchant.
    pub merchant_id: Option<String>,
    /// Restrict to one transaction status, in its serialised form
    /// (for example `"completed"` or `"partially_refunded"`).
    pub status: Option<String>,
    /// Restrict to one gateway identifier (for example `"bkash"`).
    pub gateway: Option<String>,
    /// Maximum number of transactions to return. Defaults to 50 and
    /// is capped at 200.
    pub limit: Option<usize>,
    /// Wave 11: 1-based page number. When `page` or `per_page` is
    /// supplied, paging (see `pagination::apply_page`) replaces the
    /// `limit` behaviour.
    pub page: Option<u32>,
    /// Wave 11: page size (clamped to 1..=100 by `PageQuery`).
    pub per_page: Option<u32>,
}

/// Query parameters for `list_refunds`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListRefundsQuery {
    /// Restrict to one merchant, with the same owner-scoping rule as
    /// [`ListQ::merchant_id`].
    pub merchant_id: Option<String>,
}

/// Request body for `create_refund`.
#[derive(Debug, Clone, Deserialize)]
pub struct RefundReq {
    /// Amount to refund, in the transaction currency's minor unit.
    /// When omitted, the full remaining refundable amount of the
    /// transaction is refunded.
    pub amount_minor: Option<i64>,
    /// Optional free-text reason for the refund.
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Ownership helpers
// ---------------------------------------------------------------------------

/// Whether the caller is a platform admin (sees every merchant).
fn is_admin(auth: &AuthUser) -> bool {
    auth.is_admin()
}

/// The merchant the caller acts for, if they are merchant-scoped.
fn caller_merchant(auth: &AuthUser) -> Option<&str> {
    auth.merchant_id.as_deref()
}

/// Whether the caller may access a record owned by `merchant_id`:
/// admins may access everything, everyone else only their own
/// merchant's records.
fn may_access(auth: &AuthUser, merchant_id: &str) -> bool {
    is_admin(auth) || caller_merchant(auth) == Some(merchant_id)
}

/// The effective merchant filter for a list call.
///
/// * Admin: the requested `merchant_id` filter, if any.
/// * Non-admin: always their own merchant, whatever was requested —
///   a request naming another merchant simply matches nothing,
///   because the two filters are combined by the caller.
fn effective_merchant_filter<'a>(
    auth: &'a AuthUser,
    requested: Option<&'a str>,
) -> Option<&'a str> {
    if is_admin(auth) {
        requested
    } else {
        caller_merchant(auth)
    }
}

/// Serialised (snake_case) form of a transaction status, used to
/// compare a status against the `status` query filter without
/// depending on the variant's `Debug` spelling.
fn status_label(status: TxStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------

/// List transactions, newest first.
///
/// Owner-scoped: a non-admin caller only sees their own merchant's
/// transactions; an admin sees all of them and may narrow the list
/// with the `merchant_id` query parameter. The `status` and
/// `gateway` parameters filter further. At most `limit`
/// transactions are returned (default 50, maximum 200).
pub async fn list_transactions(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListQ>,
) -> Result<Json<Vec<Transaction>>, ApiError> {
    let limit = match query.limit {
        None | Some(0) => 50,
        Some(limit) => limit.min(200),
    };
    let merchant_filter = effective_merchant_filter(&auth, query.merchant_id.as_deref());
    let status_filter = query
        .status
        .as_deref()
        .map(|status| status.trim().to_ascii_lowercase());
    let gateway_filter = query
        .gateway
        .as_deref()
        .map(|gateway| gateway.trim().to_ascii_lowercase());

    let transactions = state.transactions.read().await;
    let mut items: Vec<Transaction> = transactions
        .values()
        .filter(|tx| may_access(&auth, &tx.merchant_id))
        .filter(|tx| match merchant_filter {
            Some(merchant_id) => tx.merchant_id == merchant_id,
            None => true,
        })
        // A non-admin who explicitly asked for another merchant's
        // transactions gets an empty list, not that merchant's data.
        .filter(|tx| match query.merchant_id.as_deref() {
            Some(requested) if !is_admin(&auth) => tx.merchant_id == requested,
            _ => true,
        })
        .filter(|tx| match status_filter.as_deref() {
            Some(status) => status_label(tx.status) == status,
            None => true,
        })
        .filter(|tx| match gateway_filter.as_deref() {
            Some(gateway) => tx.gateway.to_ascii_lowercase() == gateway,
            None => true,
        })
        .cloned()
        .collect();
    // Newest first.
    items.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    if query.page.is_some() || query.per_page.is_some() {
        items = crate::pagination::apply_page(items, query.page, query.per_page);
    } else {
        items.truncate(limit);
    }
    Ok(Json(items))
}

/// Fetch a single transaction by id.
///
/// Only the merchant who owns the transaction, or an admin, may read
/// it; anyone else is refused with `403 Forbidden`.
pub async fn get_transaction(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Transaction>, ApiError> {
    let transactions = state.transactions.read().await;
    let tx = transactions
        .get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("transaction {id} not found")))?;
    if !may_access(&auth, &tx.merchant_id) {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    Ok(Json(tx.clone()))
}

// ---------------------------------------------------------------------------
// Refunds
// ---------------------------------------------------------------------------

/// Create a refund against the transaction named in the path.
///
/// The transaction must be `Completed` or `PartiallyRefunded`;
/// refunding a transaction in any other state is a `409 Conflict`.
/// The amount defaults to the transaction's remaining refundable
/// balance and is validated with
/// [`miraclepay_domain::refund::validate_refund`] against the total
/// already refunded (refunds in `Succeeded` or `Processing` state),
/// with a validation failure reported as `400 Bad Request`.
///
/// The refund is recorded as `Pending`, then submitted to the
/// gateway that processed the transaction (looked up through the
/// merchant's gateway registry) when that gateway is registered:
///
/// * on success the refund moves to `Processing` and stores the
///   provider's refund identifier;
/// * when the gateway does not support refunds (or is not
///   registered / configured for the merchant) the refund stays
///   `Pending` and the response carries a `"manual processing"`
///   note, because a human must return the money out of band.
///
/// The transaction's own status is advanced to `PartiallyRefunded`,
/// or to `Refunded` when this refund covers the full remaining
/// balance, and a `payment.refunded` webhook event is queued for the
/// merchant. The created refund is returned with `201 Created`.
pub async fn create_refund(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(tx_id): Path<String>,
    Json(body): Json<RefundReq>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Load the transaction and check ownership before revealing or
    // changing anything about it. The lock is dropped before the
    // (potentially slow) gateway call below.
    let tx = {
        let transactions = state.transactions.read().await;
        let tx = transactions
            .get(&tx_id)
            .ok_or_else(|| ApiError::NotFound(format!("transaction {tx_id} not found")))?;
        if !may_access(&auth, &tx.merchant_id) {
            return Err(ApiError::Forbidden("forbidden".to_string()));
        }
        tx.clone()
    };

    if !matches!(tx.status, TxStatus::Completed | TxStatus::PartiallyRefunded) {
        return Err(ApiError::Conflict(format!(
            "transaction {tx_id} is not refundable in its current state"
        )));
    }

    // Total already on its way back to the customer: refunds that
    // succeeded plus refunds the gateway is still processing.
    let already_refunded: i64 = {
        let refunds = state.refunds.read().await;
        refunds
            .values()
            .filter(|refund| refund.transaction_id == tx_id)
            .filter(|refund| {
                matches!(
                    refund.status,
                    RefundStatus::Succeeded | RefundStatus::Processing
                )
            })
            .map(|refund| refund.amount.amount_minor)
            .sum()
    };

    let total_minor = tx.amount.amount_minor;
    let amount_minor = body.amount_minor.unwrap_or(total_minor - already_refunded);
    validate_refund(total_minor, already_refunded, amount_minor)
        .map_err(|error: DomainError| ApiError::BadRequest(error.to_string()))?;

    let mut refund = Refund::new(
        refund_id(),
        tx_id.clone(),
        tx.merchant_id.clone(),
        Money::new(amount_minor, tx.amount.currency),
        body.reason.clone(),
    );

    // Submit the refund to the gateway that took the payment, if one
    // is registered for this merchant. `provider_ref` is the
    // provider's own transaction reference when we have it, falling
    // back to our transaction id.
    let gateway_request = RefundRequest {
        provider_ref: tx.provider_trx_id.clone().unwrap_or_else(|| tx.id.clone()),
        amount_minor,
        reason: body.reason.clone(),
    };
    let registry = state.registry_for(&tx.merchant_id);
    let gateway = registry.get_by_str(&tx.gateway);
    let mut manual_processing = false;
    match gateway {
        Some(gateway) => match gateway.refund(&gateway_request).await {
            Ok(result) => {
                refund.status = RefundStatus::Processing;
                refund.provider_refund_id = Some(result.provider_refund_id);
            }
            Err(GatewayError::Unsupported(_)) | Err(GatewayError::NotConfigured(_)) => {
                // No provider mechanism to reverse this payment:
                // the refund stays Pending for a human to settle.
                manual_processing = true;
            }
            Err(error) => {
                // The provider call failed for another reason. Keep
                // the validated refund on record as Pending and let
                // it be settled manually rather than losing it.
                tracing_note(&error);
                manual_processing = true;
            }
        },
        None => {
            manual_processing = true;
        }
    }

    // Wave 17: the transaction's status reflects *settled* refunds
    // only. This new refund is Pending/Processing, so recomputing
    // leaves the status unchanged until the refund actually settles
    // (see `settle_refund`) — previously a merely-created refund
    // flipped the transaction straight to Refunded.
    recompute_tx_refund_status(&state, &tx_id).await;

    // Persist the refund itself.
    {
        let mut refunds = state.refunds.write().await;
        refunds.insert(refund.id.clone(), refund.clone());
    }

    // Tell the merchant, through the webhook pipeline, that a refund
    // was issued against one of their payments.
    {
        let event = WebhookEvent {
            id: webhook_event_id(),
            merchant_id: tx.merchant_id.clone(),
            event_type: "payment.refunded".to_string(),
            payload: json!({
                "transaction_id": tx.id,
                "refund_id": refund.id,
                "merchant_id": tx.merchant_id,
                "amount_minor": refund.amount.amount_minor,
                "currency": refund.amount.currency.code(),
                "status": refund.status,
                // Whether issued refunds (incl. this one) cover the
                // full amount; settlement state is separate (Wave 17).
                "refunds_cover_full_amount": already_refunded + amount_minor >= total_minor,
            }),
            created_at: Utc::now(),
        };
        let mut events = state.webhook_events.write().await;
        events.push(event);
    }

    let mut response = serde_json::to_value(&refund)
        .map_err(|error| ApiError::Internal(format!("serialising refund: {error}")))?;
    if manual_processing {
        if let Some(object) = response.as_object_mut() {
            object.insert(
                "note".to_string(),
                Value::String("manual processing".to_string()),
            );
        }
    }
    Ok((StatusCode::CREATED, Json(response)))
}

/// Small indirection so a gateway failure is noted in one place
/// (and so this module needs no logging dependency of its own).
fn tracing_note(error: &GatewayError) {
    eprintln!("miraclepay: gateway refund failed, falling back to manual processing: {error}");
}

/// Recompute a transaction's refund status from its *settled*
/// (Succeeded) refunds (Wave 17): none settled → `Completed`, some
/// but not all → `PartiallyRefunded`, fully covered → `Refunded`.
/// Transactions outside the refundable family are left untouched.
pub async fn recompute_tx_refund_status(state: &AppState, tx_id: &str) {
    let settled: i64 = {
        let refunds = state.refunds.read().await;
        refunds
            .values()
            .filter(|r| r.transaction_id == tx_id && r.status == RefundStatus::Succeeded)
            .map(|r| r.amount.amount_minor)
            .sum()
    };
    let mut transactions = state.transactions.write().await;
    if let Some(stored) = transactions.get_mut(tx_id) {
        if matches!(
            stored.status,
            TxStatus::Completed | TxStatus::PartiallyRefunded | TxStatus::Refunded
        ) {
            stored.status = if settled <= 0 {
                TxStatus::Completed
            } else if settled >= stored.amount.amount_minor {
                TxStatus::Refunded
            } else {
                TxStatus::PartiallyRefunded
            };
            stored.updated_at = Utc::now();
        }
    }
}

/// Body of `POST /refunds/{id}/settle`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SettleRefundReq {
    /// `"succeeded"` or `"failed"`.
    pub outcome: String,
}

/// `POST /refunds/{id}/settle` — settle an in-flight refund
/// (Wave 17): the step that makes the refund flow end-to-end.
///
/// Manual gateways leave refunds Pending; gateway refunds sit in
/// Processing until the provider's final verdict is known. This
/// endpoint records that verdict (merchant-owner or admin only),
/// moves the refund to its terminal state, recomputes the parent
/// transaction's status from settled refunds, audits the action,
/// and emits a `refund.succeeded` / `refund.failed` webhook event.
pub async fn settle_refund(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SettleRefundReq>,
) -> Result<Json<Value>, ApiError> {
    let outcome = match body.outcome.trim().to_ascii_lowercase().as_str() {
        "succeeded" => RefundStatus::Succeeded,
        "failed" => RefundStatus::Failed,
        other => {
            return Err(ApiError::BadRequest(format!(
                "outcome must be \"succeeded\" or \"failed\", got \"{other}\""
            )))
        }
    };

    let refund = {
        let mut refunds = state.refunds.write().await;
        let refund = refunds
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("refund {id} not found")))?;
        if !may_access(&auth, &refund.merchant_id) {
            return Err(ApiError::Forbidden("forbidden".to_string()));
        }
        if !refund.status.is_in_flight() {
            return Err(ApiError::Conflict(format!(
                "refund {id} is already {}",
                refund.status
            )));
        }
        refund.status = outcome;
        refund.clone()
    };

    recompute_tx_refund_status(&state, &refund.transaction_id).await;

    state.audit(
        Some(auth.user_id.clone()),
        Some(refund.merchant_id.clone()),
        if outcome == RefundStatus::Succeeded {
            "refund.succeeded"
        } else {
            "refund.failed"
        },
        "refund",
        Some(refund.id.clone()),
    );

    {
        let event = WebhookEvent {
            id: webhook_event_id(),
            merchant_id: refund.merchant_id.clone(),
            event_type: if outcome == RefundStatus::Succeeded {
                "refund.succeeded".to_string()
            } else {
                "refund.failed".to_string()
            },
            payload: json!({
                "refund_id": refund.id,
                "transaction_id": refund.transaction_id,
                "merchant_id": refund.merchant_id,
                "amount_minor": refund.amount.amount_minor,
                "currency": refund.amount.currency.code(),
            }),
            created_at: Utc::now(),
        };
        state.webhook_events.write().await.push(event);
    }

    Ok(Json(json!({
        "id": refund.id,
        "status": refund.status,
        "transaction_id": refund.transaction_id,
    })))
}

/// List refunds, newest first.
///
/// Owner-scoped exactly like [`list_transactions`]: a non-admin
/// caller only sees their own merchant's refunds; an admin sees all
/// of them and may narrow the list with the `merchant_id` query
/// parameter.
pub async fn list_refunds(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListRefundsQuery>,
) -> Result<Json<Vec<Refund>>, ApiError> {
    let merchant_filter = effective_merchant_filter(&auth, query.merchant_id.as_deref());

    let refunds = state.refunds.read().await;
    let mut items: Vec<Refund> = refunds
        .values()
        .filter(|refund| may_access(&auth, &refund.merchant_id))
        .filter(|refund| match merchant_filter {
            Some(merchant_id) => refund.merchant_id == merchant_id,
            None => true,
        })
        .filter(|refund| match query.merchant_id.as_deref() {
            Some(requested) if !is_admin(&auth) => refund.merchant_id == requested,
            _ => true,
        })
        .cloned()
        .collect();
    // Newest first.
    items.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    Ok(Json(items))
}
