//! Payment handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! Endpoints:
//!
//! * `create_payment`  — authenticated. Starts a payment through one
//!   of the merchant's configured gateways and records the resulting
//!   transaction in `Processing`.
//! * `get_payment`     — authenticated. Fetches one transaction the
//!   caller owns (or any transaction, for an admin).
//! * `list_payments`   — authenticated. Lists the caller's
//!   transactions, newest first, optionally filtered by merchant
//!   and / or status.
//! * `checkout_details` — **public** (no auth). The minimal,
//!   non-sensitive view of a transaction that the hosted checkout
//!   page needs in order to render itself.
//! * `submit_checkout` — **public** (no auth). The hosted checkout
//!   page reports the sender number and the provider transaction id
//!   for a still-`Pending` transaction, moving it to `Processing`.
//!
//! Every handler is a thin, individually callable unit: it validates
//! ownership and input, delegates the actual payment initiation to
//! the merchant's gateway registry, and stores results in
//! [`AppState`].

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::ids;
use miraclepay_domain::money::{Currency, Money};
use miraclepay_domain::transaction::{Transaction, TxStatus};
use miraclepay_domain::webhook::WebhookEvent;
use miraclepay_gateways::types::{GatewayError, PaymentRequest};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / query types
// ---------------------------------------------------------------------------

/// Body of `create_payment`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePaymentReq {
    /// Merchant the payment is collected for. Must exist and must
    /// belong to the caller (admins may create for any merchant).
    pub merchant_id: String,
    /// Gateway identifier, e.g. `"bkash"` or `"stripe"`. Resolved
    /// through the merchant's gateway registry.
    pub gateway: String,
    /// Amount to collect, in the currency's minor unit.
    pub amount_minor: i64,
    /// ISO 4217 currency code, e.g. `"BDT"`.
    pub currency: String,
    /// Customer's email address, when known.
    pub customer_email: Option<String>,
    /// Customer's phone number, when known.
    pub customer_phone: Option<String>,
    /// Caller's own reference for this payment (order / invoice id).
    pub reference: Option<String>,
}

/// Query parameters of `list_payments`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQ {
    /// Restrict to this merchant.
    pub merchant_id: Option<String>,
    /// Restrict to this status, e.g. `"pending"` or `"completed"`.
    pub status: Option<String>,
}

/// Body of `submit_checkout`.
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitReq {
    /// Number the customer sent the payment from.
    pub sender_number: String,
    /// Transaction identifier assigned by the provider, as shown on
    /// the customer's confirmation.
    pub provider_trx_id: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Whether `auth` is a platform administrator.
fn is_admin(auth: &AuthUser) -> bool {
    auth.role.eq_ignore_ascii_case("admin")
}

/// Whether `auth` owns `merchant_id`: directly, through the
/// merchant's `owner_user_id`, or by being an admin.
async fn owns_merchant(state: &AppState, auth: &AuthUser, merchant_id: &str) -> bool {
    if is_admin(auth) {
        return true;
    }
    if auth.merchant_id.as_deref() == Some(merchant_id) {
        return true;
    }
    let merchants = state.merchants.read().await;
    merchants
        .get(merchant_id)
        .map(|merchant| merchant.owner_user_id == auth.user_id)
        .unwrap_or(false)
}

/// Parse a status filter string into a [`TxStatus`].
fn parse_status(raw: &str) -> Option<TxStatus> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pending" => Some(TxStatus::Pending),
        "processing" => Some(TxStatus::Processing),
        "completed" => Some(TxStatus::Completed),
        "failed" => Some(TxStatus::Failed),
        "refunded" => Some(TxStatus::Refunded),
        "partially_refunded" => Some(TxStatus::PartiallyRefunded),
        "expired" => Some(TxStatus::Expired),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Authenticated handlers
// ---------------------------------------------------------------------------

/// Create (initiate) a payment.
///
/// Validates that the merchant exists and belongs to the caller,
/// parses the currency, resolves the gateway from the merchant's
/// registry, builds a `Pending` transaction, asks the gateway to
/// initiate the payment, then stores the transaction as
/// `Processing` with the provider reference attached and records a
/// `payment.created` webhook event.
///
/// Returns `201` with `{ transaction, initiated: { redirect_url,
/// instructions } }`.
pub async fn create_payment(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreatePaymentReq>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // The merchant must exist …
    {
        let merchants = state.merchants.read().await;
        if !merchants.contains_key(&req.merchant_id) {
            return Err(ApiError::NotFound(format!(
                "merchant not found: {}",
                req.merchant_id
            )));
        }
    }
    // … and belong to the caller (admins excepted).
    if !owns_merchant(&state, &auth, &req.merchant_id).await {
        return Err(ApiError::Forbidden(
            "merchant does not belong to the caller".to_string(),
        ));
    }

    if req.amount_minor <= 0 {
        return Err(ApiError::BadRequest(
            "amount_minor must be greater than zero".to_string(),
        ));
    }

    let currency = Currency::from_code(&req.currency)
        .ok_or_else(|| ApiError::BadRequest(format!("unsupported currency: {}", req.currency)))?;
    let amount = Money::new(req.amount_minor, currency);

    // Resolve the gateway through this merchant's registry so a
    // merchant can only pay through gateways configured for them.
    let registry = state.registry_for(&req.merchant_id);
    let gateway = registry
        .get_by_str(&req.gateway)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown gateway: {}", req.gateway)))?;

    let now = Utc::now();
    let mut transaction = Transaction {
        id: ids::transaction_id(),
        merchant_id: req.merchant_id.clone(),
        gateway: req.gateway.trim().to_ascii_lowercase(),
        amount,
        status: TxStatus::Pending,
        customer_email: req.customer_email.clone(),
        customer_phone: req.customer_phone.clone(),
        provider_trx_id: None,
        sender_number: None,
        reference: req.reference.clone(),
        metadata: Value::Null,
        created_at: now,
        updated_at: now,
    };

    let payment_request = PaymentRequest {
        merchant_id: req.merchant_id.clone(),
        amount,
        reference: req
            .reference
            .clone()
            .unwrap_or_else(|| transaction.id.clone()),
        customer_name: None,
        customer_email: req.customer_email.clone(),
        customer_phone: req.customer_phone.clone(),
        callback_url: None,
        metadata: Value::Null,
    };

    let initiated = gateway.initiate(&payment_request).await.map_err(|err| {
        match err {
            // An unconfigured gateway is a fixable request-side
            // problem, surfaced with the gateway's own message.
            GatewayError::NotConfigured(message) => ApiError::BadRequest(message),
            other => ApiError::BadRequest(other.to_string()),
        }
    })?;

    transaction.status = TxStatus::Processing;
    transaction.provider_trx_id = Some(initiated.provider_ref.clone());
    transaction.updated_at = Utc::now();

    state
        .transactions
        .write()
        .await
        .insert(transaction.id.clone(), transaction.clone());
    state.assess_risk(&transaction).await;

    state.webhook_events.write().await.push(WebhookEvent {
        id: ids::webhook_event_id(),
        merchant_id: transaction.merchant_id.clone(),
        event_type: "payment.created".to_string(),
        payload: json!({
            "transaction_id": transaction.id,
            "merchant_id": transaction.merchant_id,
            "gateway": transaction.gateway,
            "amount_minor": transaction.amount.amount_minor,
            "currency": transaction.amount.currency.code(),
            "status": transaction.status,
            "provider_trx_id": transaction.provider_trx_id,
            "reference": transaction.reference,
        }),
        created_at: Utc::now(),
    });

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "transaction": transaction,
            "initiated": {
                "redirect_url": initiated.redirect_url,
                "instructions": initiated.instructions,
            },
        })),
    ))
}

/// Fetch one transaction by id.
///
/// The caller must own the transaction's merchant (admins may fetch
/// any transaction).
pub async fn get_payment(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let transaction = {
        let transactions = state.transactions.read().await;
        transactions.get(&id).cloned()
    }
    .ok_or_else(|| ApiError::NotFound(format!("transaction not found: {id}")))?;

    if !owns_merchant(&state, &auth, &transaction.merchant_id).await {
        return Err(ApiError::Forbidden(
            "transaction does not belong to the caller".to_string(),
        ));
    }

    Ok(Json(json!({ "transaction": transaction })))
}

/// List transactions visible to the caller, newest first.
///
/// Optional `merchant_id` and `status` query filters narrow the
/// result; at most 100 transactions are returned.
pub async fn list_payments(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListQ>,
) -> Result<Json<Value>, ApiError> {
    let status_filter = match query.status.as_deref() {
        Some(raw) => Some(
            parse_status(raw)
                .ok_or_else(|| ApiError::BadRequest(format!("unknown status filter: {raw}")))?,
        ),
        None => None,
    };

    // Non-admin callers only ever see their own merchants' payments,
    // even when they ask for another merchant explicitly.
    let visible_merchants: Option<Vec<String>> = if is_admin(&auth) {
        None
    } else {
        let merchants = state.merchants.read().await;
        let mut ids: Vec<String> = merchants
            .values()
            .filter(|merchant| merchant.owner_user_id == auth.user_id)
            .map(|merchant| merchant.id.clone())
            .collect();
        if let Some(own) = auth.merchant_id.clone() {
            if !ids.contains(&own) {
                ids.push(own);
            }
        }
        Some(ids)
    };

    let mut transactions: Vec<Transaction> = {
        let store = state.transactions.read().await;
        store.values().cloned().collect()
    };

    transactions.retain(|tx| {
        if let Some(merchant_id) = query.merchant_id.as_deref() {
            if tx.merchant_id != merchant_id {
                return false;
            }
        }
        if let Some(status) = status_filter {
            if tx.status != status {
                return false;
            }
        }
        if let Some(visible) = visible_merchants.as_ref() {
            if !visible.iter().any(|id| id == &tx.merchant_id) {
                return false;
            }
        }
        true
    });

    transactions.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    transactions.truncate(100);

    Ok(Json(json!({ "transactions": transactions })))
}

// ---------------------------------------------------------------------------
// Public hosted-checkout handlers (no auth)
// ---------------------------------------------------------------------------

/// Public checkout summary for one transaction.
///
/// Returns only what the hosted checkout page needs to render: the
/// transaction id, the display amount, the gateway, the current
/// status, and the merchant's business name. No customer contact
/// details or internal fields are exposed.
pub async fn checkout_details(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let transaction = {
        let transactions = state.transactions.read().await;
        transactions.get(&id).cloned()
    }
    .ok_or_else(|| ApiError::NotFound(format!("transaction not found: {id}")))?;

    let business_name = {
        let merchants = state.merchants.read().await;
        merchants
            .get(&transaction.merchant_id)
            .map(|merchant| merchant.business_name.clone())
            .unwrap_or_default()
    };

    Ok(Json(json!({
        "transaction_id": transaction.id,
        "amount": transaction.amount.format_display(),
        "amount_display": transaction.amount.format_display(),
        "amount_minor": transaction.amount.amount_minor,
        "currency": transaction.amount.currency.code(),
        "gateway": transaction.gateway,
        "status": transaction.status,
        "merchant": business_name.clone(),
        "merchant_name": business_name.clone(),
        "business_name": business_name,
    })))
}

/// Public checkout submission.
///
/// The customer reports the number they sent the payment from and
/// the provider transaction id from their confirmation. Only a
/// `Pending` transaction may be submitted; submitting one that has
/// already moved on is a conflict. On success the transaction is
/// `Processing`, awaiting verification.
pub async fn submit_checkout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SubmitReq>,
) -> Result<Json<Value>, ApiError> {
    if req.sender_number.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "sender_number must not be empty".to_string(),
        ));
    }
    if req.provider_trx_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "provider_trx_id must not be empty".to_string(),
        ));
    }

    let mut transactions = state.transactions.write().await;
    let transaction = transactions
        .get_mut(&id)
        .ok_or_else(|| ApiError::NotFound(format!("transaction not found: {id}")))?;

    if transaction.status != TxStatus::Pending {
        return Err(ApiError::Conflict(format!(
            "transaction {id} is not pending and cannot be submitted"
        )));
    }

    transaction.sender_number = Some(req.sender_number.trim().to_string());
    transaction.provider_trx_id = Some(req.provider_trx_id.trim().to_string());
    transaction.status = TxStatus::Processing;
    transaction.updated_at = Utc::now();

    Ok(Json(json!({ "transaction": transaction.clone() })))
}
