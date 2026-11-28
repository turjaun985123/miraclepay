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
use miraclepay_gateways::types::{GatewayConfig, GatewayError, GatewayId, PaymentRequest};

use crate::auth_middleware::AuthUser;
use crate::key_auth::authenticate_api_key;
use axum::http::HeaderMap;
use miraclepay_domain::api_key::ApiScope;

/// Wave 103: a caller is either a JWT user or an API key with the
/// required scope. JWT wins when both are presented.
async fn resolve_caller(
    state: &AppState,
    auth: Option<AuthUser>,
    headers: &HeaderMap,
    scope: ApiScope,
) -> Result<AuthUser, ApiError> {
    match auth {
        Some(user) => Ok(user),
        None => authenticate_api_key(state, headers, scope).await,
    }
}
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
    /// Optional gateway the customer chose to pay through, e.g.
    /// `"bkash"`. When present and non-blank it is validated
    /// against the gateways the merchant actually offers (see
    /// [`submit_checkout`]) and, when valid, replaces the
    /// transaction's gateway before the transaction moves to
    /// `Processing`. When absent, `null`, or blank, the gateway
    /// recorded at creation is kept unchanged.
    pub gateway: Option<String>,
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

/// Gateway ids a merchant offers payers, computed with the SAME
/// rule as the public payment-link view (`public_link` in
/// `payment_links.rs`): the merchant's configured gateways when
/// they have configured any, otherwise every supported gateway in
/// their registry (unconfigured = unrestricted).
fn offered_gateway_ids(state: &AppState, merchant_id: &str) -> Vec<GatewayId> {
    let registry = state.registry_for(merchant_id);
    let configured = registry.configured_ids();
    if configured.is_empty() {
        registry.list()
    } else {
        configured
    }
}

/// Whether a gateway's `merchant_number` is a pay-to address that
/// may be shown to a payer (Stream A spec §2.1 EXPOSED set: the
/// number-based manual / SMS-verified adapters). Credential
/// gateways (stripe / paypal / razorpay) are excluded — their
/// `merchant_number` is meaningless and must never be exposed, even
/// if a stored config carries a value.
fn receiving_number_exposed(gateway: GatewayId) -> bool {
    matches!(
        gateway,
        GatewayId::Bkash
            | GatewayId::Nagad
            | GatewayId::Rocket
            | GatewayId::Upay
            | GatewayId::Surecash
            | GatewayId::Upi
            | GatewayId::Paytm
            | GatewayId::Phonepe
            | GatewayId::Easypaisa
            | GatewayId::Jazzcash
            | GatewayId::Manual
    )
}

/// Identifier shape check from the gateway's own adapter
/// (Stream A spec §2.2 rule 5): a number the adapter itself would
/// reject as invalid configuration must not be shown to a payer.
fn receiving_identifier_valid(gateway: GatewayId, value: &str) -> bool {
    match gateway {
        GatewayId::Upi => miraclepay_gateways::in_manual::valid_vpa(value),
        GatewayId::Paytm | GatewayId::Phonepe => {
            miraclepay_gateways::in_manual::valid_in_mobile(value)
                || miraclepay_gateways::in_manual::valid_vpa(value)
        }
        GatewayId::Easypaisa | GatewayId::Jazzcash => {
            miraclepay_gateways::pk_manual::valid_pk_mobile(value)
        }
        _ => true,
    }
}

/// The payer-safe receiving number for one gateway from a config
/// list — the single implementation of Stream A spec §2.2, shared
/// by every public surface (checkout scalar, checkout list, public
/// payment-link view). Returns the trimmed number iff the gateway
/// is in the EXPOSED set, a config exists (last one wins, mirroring
/// upsert semantics), the config is enabled, the number is
/// non-blank, and it passes the adapter's own identifier
/// validation. Only the number string ever leaves through here —
/// credentials and `extra` config cannot cross.
pub(crate) fn exposed_receiving_number(
    configs: &[GatewayConfig],
    gateway: GatewayId,
) -> Option<String> {
    if !receiving_number_exposed(gateway) {
        return None;
    }
    let config = configs.iter().rev().find(|c| c.gateway == gateway)?;
    if !config.enabled {
        return None;
    }
    let trimmed = config.merchant_number.as_ref()?.trim();
    if trimmed.is_empty() || !receiving_identifier_valid(gateway, trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

/// Receiving numbers for the offered gateways, in the same shape
/// as the public payment-link view (`[{gateway, number}]`): one
/// entry per offered gateway passing the §2.2 predicate
/// ([`exposed_receiving_number`]), emitted in offered (`gateways`)
/// order; offered gateways without an exposable number are
/// omitted. Only the gateway id and the number are returned —
/// credentials and `extra` config never leave the server (see
/// `checkout_details` for the payer-safety rationale).
async fn receiving_numbers_for(
    state: &AppState,
    merchant_id: &str,
    offered: &[GatewayId],
) -> Vec<Value> {
    let all_configs = state.gateway_configs.read().await;
    match all_configs.get(merchant_id) {
        Some(configs) => offered
            .iter()
            .filter_map(|gateway| {
                exposed_receiving_number(configs, *gateway).map(|number| {
                    json!({
                        "gateway": gateway.as_str(),
                        "number": number,
                    })
                })
            })
            .collect(),
        None => Vec::new(),
    }
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
    auth: Option<AuthUser>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreatePaymentReq>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let auth = resolve_caller(&state, auth, &headers, ApiScope::PaymentsWrite).await?;
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
    // Waves 118-120: ids the native registry does not know fall
    // through to the plugin lane (an enabled plugin that provides
    // the id); anything else is an unknown gateway.
    let registry = state.registry_for(&req.merchant_id);
    let gateway = registry.get_by_str(&req.gateway);
    let plugin_lane = match &gateway {
        Some(_) => false,
        None => crate::plugins::plugin_provides_gateway(&state, &req.gateway).await,
    };
    if gateway.is_none() && !plugin_lane {
        return Err(ApiError::BadRequest(format!(
            "unknown gateway: {}",
            req.gateway
        )));
    }

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

    let initiated = match &gateway {
        Some(gateway) => gateway.initiate(&payment_request).await,
        None => crate::plugins::plugin_initiate(&state, &req.gateway, &payment_request).await,
    }
    .map_err(|err| match err {
        // An unconfigured gateway is a fixable request-side
        // problem, surfaced with the gateway's own message.
        GatewayError::NotConfigured(message) => ApiError::BadRequest(message),
        other => ApiError::BadRequest(other.to_string()),
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
    auth: Option<AuthUser>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let auth = resolve_caller(&state, auth, &headers, ApiScope::PaymentsRead).await?;
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
    auth: Option<AuthUser>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListQ>,
) -> Result<Json<Value>, ApiError> {
    let auth = resolve_caller(&state, auth, &headers, ApiScope::PaymentsRead).await?;
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
/// status, the merchant's business name, and the merchant's
/// receiving number for the gateway (`receiving_number`, `null`
/// when none is configured — see the lookup below for why that one
/// config value is payer-safe). It also returns `gateways` — the
/// gateway ids the merchant offers, computed with the same rule as
/// the public payment-link view — and `receiving_numbers`, the
/// per-gateway receiving numbers for those offered gateways, so
/// the page can offer the payer a real gateway choice and update
/// the receiving number (and the gateway-specific payment
/// instructions) when the payer switches gateway. No customer
/// contact details, credentials, or other internal fields are
/// exposed.
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

    // The merchant's receiving number for this transaction's
    // gateway: the account number the payer sends money to for
    // mobile-money gateways. This is payer-safe by design — it is
    // the number the merchant publicly asks payers to pay into —
    // and it is taken ONLY from `GatewayConfig::merchant_number`
    // through the Stream A spec §2.2 predicate
    // (`exposed_receiving_number`): EXPOSED gateways only, enabled
    // config, non-blank, adapter-valid identifier. Credentials
    // (`api_key` / `api_secret`) and free-form `extra` config are
    // never exposed on this public surface. `null` whenever the
    // predicate does not pass.
    let receiving_number = {
        let all_configs = state.gateway_configs.read().await;
        GatewayId::from_str(&transaction.gateway).and_then(|gateway| {
            all_configs
                .get(&transaction.merchant_id)
                .and_then(|configs| exposed_receiving_number(configs, gateway))
        })
    };

    // Gateways offered to the payer, and the receiving number for
    // each of them — the same rule and shape as the public
    // payment-link view, so /checkout and /pay/<slug> present the
    // identical gateway choice for the same merchant.
    let offered = offered_gateway_ids(&state, &transaction.merchant_id);
    let mut gateways: Vec<String> = offered.iter().map(|id| id.as_str().to_string()).collect();
    // Plugin-lane gateways are platform-level (not per-merchant):
    // fold in every gateway a usable plugin provides so the picker
    // offers exactly the gateways `submit_checkout` accepts
    // (v0.0.4 Stream B FINDING-1). They carry no receiving number —
    // `receiving_numbers` below stays native-config only.
    for id in crate::plugins::plugin_gateway_ids(&state).await {
        if !gateways.iter().any(|g| g.eq_ignore_ascii_case(&id)) {
            gateways.push(id);
        }
    }
    let receiving_numbers = receiving_numbers_for(&state, &transaction.merchant_id, &offered).await;

    // Wave 98 correction: hosted checkout uses the same merchant
    // branding shape as the public payment-link view. Settings are
    // optional; an unbranded merchant gets null fields and the
    // platform default look.
    let branding = {
        let all_settings = state.merchant_settings.read().await;
        match all_settings.get(&transaction.merchant_id) {
            Some(settings) => json!({
                "display_name": settings.brand_name,
                "brand_name": settings.brand_name,
                "brand_color": settings.brand_color,
                "logo_url": settings.logo_url,
                "checkout_message": settings.checkout_message,
            }),
            None => json!({
                "display_name": null,
                "brand_name": null,
                "brand_color": null,
                "logo_url": null,
                "checkout_message": null,
            }),
        }
    };

    Ok(Json(json!({
        "transaction_id": transaction.id,
        "amount": transaction.amount.format_display(),
        "amount_display": transaction.amount.format_display(),
        "amount_minor": transaction.amount.amount_minor,
        "currency": transaction.amount.currency.code(),
        "gateway": transaction.gateway,
        "gateways": gateways,
        "receiving_number": receiving_number,
        "receiving_numbers": receiving_numbers,
        "status": transaction.status,
        "merchant": business_name.clone(),
        "merchant_name": business_name.clone(),
        "business_name": business_name,
        "branding": branding.clone(),
        "brand_color": branding["brand_color"],
        "logo_url": branding["logo_url"],
        "checkout_message": branding["checkout_message"],
    })))
}

/// Public checkout submission.
///
/// The customer reports the number they sent the payment from and
/// the provider transaction id from their confirmation. Only a
/// `Pending` transaction may be submitted; submitting one that has
/// already moved on is a conflict. On success the transaction is
/// `Processing`, awaiting verification.
///
/// The customer may also name the gateway they actually paid
/// through (`gateway`, optional). The chosen gateway matters: it
/// drives the payment instructions the payer followed and the
/// settlement matching that later pairs the incoming money with
/// this transaction, so it is recorded on the transaction before
/// the status moves to `Processing` — and because only a `Pending`
/// transaction can be submitted, it can never be changed after
/// submission.
///
/// Validation (Stream B spec §2.3, in this order — sender, TrxID,
/// existence, status, then gateway; a rejected gateway stores
/// nothing, so the payer can correct the choice and resubmit):
///
/// * `gateway` absent, `null`, or blank after trimming keeps the
///   creation gateway byte-identical.
/// * A present, non-blank gateway is normalized
///   (`trim().to_ascii_lowercase()`). Keeping the creation gateway
///   (candidate == current, normalized) is always accepted — the
///   no-op rule — and canonicalises a verbatim-stored spelling.
/// * A gateway an enabled plugin provides is also accepted,
///   mirroring `create_payment`'s plugin lane — for plugin ids
///   only; an id that parses as a native gateway must be in the
///   merchant's offered set (B6 F4 guard).
/// * Otherwise an id [`GatewayId::from_str`] does not know is
///   `400 "unknown gateway: {candidate}"`, and a known id outside
///   the merchant's offered set (the same rule as
///   `checkout_details`) is `400 "gateway is not offered for this
///   merchant: {candidate}"`. Validation prevents pointing a
///   payment at a gateway the merchant doesn't offer, where its
///   money could never be matched or settled.
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

    // Snapshot the transaction under a read lock: existence (404)
    // and status (409) are checked BEFORE gateway validation, and
    // the offered set depends only on the merchant id, so the
    // (async) gateway validation runs without holding the write
    // lock. The write-lock status check below stays authoritative.
    let (merchant_id, status, current_gateway) = {
        let transactions = state.transactions.read().await;
        let transaction = transactions
            .get(&id)
            .ok_or_else(|| ApiError::NotFound(format!("transaction not found: {id}")))?;
        (
            transaction.merchant_id.clone(),
            transaction.status,
            transaction.gateway.clone(),
        )
    };
    if status != TxStatus::Pending {
        return Err(ApiError::Conflict(format!(
            "transaction {id} is not pending and cannot be submitted"
        )));
    }

    // Resolve the optional gateway choice to the value to store.
    let chosen_gateway: Option<String> = match req.gateway.as_deref() {
        Some(raw) if !raw.trim().is_empty() => {
            let candidate = raw.trim().to_ascii_lowercase();
            let current = current_gateway.trim().to_ascii_lowercase();
            if candidate == current {
                // No-op rule: keeping the creation gateway is never
                // an error; store the canonical spelling when the
                // id is a native one.
                Some(
                    GatewayId::from_str(&candidate)
                        .map(|gateway| gateway.as_str().to_string())
                        .unwrap_or(candidate),
                )
            } else if GatewayId::from_str(&candidate).is_none()
                && crate::plugins::plugin_provides_gateway(&state, &candidate).await
            {
                // Plugin lane — plugin ids only. An id that parses
                // as a native gateway must go through the offered
                // set below, so a plugin can never shadow a native
                // gateway into an unoffered switch (B6 F4).
                Some(candidate)
            } else {
                match GatewayId::from_str(&candidate) {
                    None => {
                        return Err(ApiError::BadRequest(format!(
                            "unknown gateway: {candidate}"
                        )));
                    }
                    Some(gateway) => {
                        let offered = offered_gateway_ids(&state, &merchant_id);
                        if !offered.contains(&gateway) {
                            return Err(ApiError::BadRequest(format!(
                                "gateway is not offered for this merchant: {candidate}"
                            )));
                        }
                        Some(gateway.as_str().to_string())
                    }
                }
            }
        }
        // Absent, null, or blank: keep the creation gateway.
        _ => None,
    };

    let mut transactions = state.transactions.write().await;
    let transaction = transactions
        .get_mut(&id)
        .ok_or_else(|| ApiError::NotFound(format!("transaction not found: {id}")))?;

    if transaction.status != TxStatus::Pending {
        return Err(ApiError::Conflict(format!(
            "transaction {id} is not pending and cannot be submitted"
        )));
    }

    if let Some(chosen) = chosen_gateway {
        transaction.gateway = chosen;
    }
    transaction.sender_number = Some(req.sender_number.trim().to_string());
    transaction.provider_trx_id = Some(req.provider_trx_id.trim().to_string());
    transaction.status = TxStatus::Processing;
    transaction.updated_at = Utc::now();

    Ok(Json(json!({ "transaction": transaction.clone() })))
}
