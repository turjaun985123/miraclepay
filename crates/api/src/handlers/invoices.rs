//! Invoice handlers for MiraclePay v0.0.1.
//!
//! Original, clean-room implementation.
//!
//! Endpoints (mounted by the API router under `/invoices`):
//!
//! - `POST /invoices` — create a draft invoice for an owned merchant.
//! - `GET /invoices` — list invoices of the caller's merchants,
//!   optionally filtered by `merchant_id` and/or `status`.
//! - `GET /invoices/{id}` — fetch one invoice of the caller's merchants.
//! - `POST /invoices/{id}/send` — move a draft invoice to `sent` and
//!   queue an email notification for the customer.
//! - `POST /invoices/{id}/pay` — **public** (no authentication): pay a
//!   `sent` or `overdue` invoice, creating a processing transaction and
//!   marking the invoice paid.
//! - `POST /invoices/{id}/cancel` — cancel an invoice that has not been
//!   paid.
//!
//! All amounts are integer minor units. An invoice's total is never
//! stored as a separate field: it is always derived from the line
//! items, discount and tax rate through [`Invoice::total`], so a
//! response can never disagree with the invoice it describes.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use miraclepay_domain::ids::{invoice_id, new_id, transaction_id};
use miraclepay_domain::invoice::{Invoice, InvoiceItem, InvoiceStatus};
use miraclepay_domain::money::Currency;
use miraclepay_domain::notification::{invoice_sent_body, Notification, NotificationChannel};
use miraclepay_domain::transaction::{Transaction, TxStatus};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / response payloads
// ---------------------------------------------------------------------------

/// Body of `POST /invoices`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvoiceRequest {
    /// Merchant issuing the invoice. Must belong to the caller.
    pub merchant_id: String,
    /// Name of the customer being billed.
    pub customer_name: String,
    /// Email address of the customer being billed.
    pub customer_email: String,
    /// Line items making up the invoice. Must not be empty.
    pub items: Vec<InvoiceItem>,
    /// Tax rate in basis points (`10_000` = 100%), applied to the
    /// subtotal after the discount.
    pub tax_bps: u32,
    /// Flat discount in minor units, subtracted before tax.
    pub discount_minor: i64,
    /// ISO 4217 currency code, e.g. `"BDT"`.
    pub currency: String,
    /// Days from creation until payment is due. `None` (or omitted)
    /// means the invoice has no due date.
    pub due_days: Option<i64>,
}

/// Query parameters of `GET /invoices`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListInvoicesQuery {
    /// Restrict the result to a single merchant.
    pub merchant_id: Option<String>,
    /// Restrict the result to a single lifecycle status, spelled as in
    /// the domain type: `draft`, `sent`, `paid`, `overdue`,
    /// `cancelled` or `void`.
    pub status: Option<String>,
}

/// Body of `POST /invoices/{id}/pay`.
#[derive(Debug, Clone, Deserialize)]
pub struct PayInvoiceRequest {
    /// Gateway the customer pays through, e.g. `"bkash"`.
    pub gateway: String,
    /// Number the payment was sent from, for sender-verified gateways.
    pub sender_number: Option<String>,
}

/// An [`Invoice`] together with its computed amounts.
///
/// The invoice itself is flattened into the response, so every field of
/// the stored invoice is present alongside `subtotal_minor`,
/// `tax_minor` and `total_minor`.
#[derive(Debug, Clone, Serialize)]
pub struct InvoiceView {
    /// The stored invoice, flattened into the response object.
    #[serde(flatten)]
    pub invoice: Invoice,
    /// Sum of the line items, in minor units, before discount and tax.
    pub subtotal_minor: i64,
    /// Tax owed, in minor units.
    pub tax_minor: i64,
    /// Grand total payable, in minor units.
    pub total_minor: i64,
    /// Grand total payable, formatted for display, e.g. `"BDT 2,300.00"`.
    pub total_display: String,
}

impl From<&Invoice> for InvoiceView {
    fn from(invoice: &Invoice) -> Self {
        Self {
            invoice: invoice.clone(),
            subtotal_minor: invoice.subtotal_minor(),
            tax_minor: invoice.tax_minor(),
            total_minor: invoice.total_minor(),
            total_display: invoice.total().format_display(),
        }
    }
}

impl From<Invoice> for InvoiceView {
    fn from(invoice: Invoice) -> Self {
        InvoiceView::from(&invoice)
    }
}

/// Body returned by a successful `POST /invoices/{id}/pay`.
#[derive(Debug, Clone, Serialize)]
pub struct PayInvoiceResponse {
    /// Identifier of the transaction created for this payment.
    pub transaction_id: String,
    /// Identifier of the invoice that was paid.
    pub invoice_id: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a `status` query value into an [`InvoiceStatus`].
fn parse_status(raw: &str) -> Result<InvoiceStatus, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "draft" => Ok(InvoiceStatus::Draft),
        "sent" => Ok(InvoiceStatus::Sent),
        "paid" => Ok(InvoiceStatus::Paid),
        "overdue" => Ok(InvoiceStatus::Overdue),
        "cancelled" => Ok(InvoiceStatus::Cancelled),
        "void" => Ok(InvoiceStatus::Void),
        other => Err(ApiError::BadRequest(format!(
            "unknown invoice status: {other}"
        ))),
    }
}

/// Fail unless `merchant_id` names a merchant owned by `auth`.
///
/// A merchant that does not exist is `404`; one that exists but belongs
/// to another user is `403`, so a caller can never act on — or probe
/// through — somebody else's merchant by guessing its id here.
async fn ensure_merchant_owned(
    state: &AppState,
    auth: &AuthUser,
    merchant_id: &str,
) -> Result<(), ApiError> {
    let merchants = state.merchants.read().await;
    let merchant = merchants
        .get(merchant_id)
        .ok_or_else(|| ApiError::NotFound(format!("merchant not found: {merchant_id}")))?;
    if merchant.owner_user_id != auth.user_id {
        return Err(ApiError::Forbidden(
            "merchant does not belong to the authenticated user".to_string(),
        ));
    }
    Ok(())
}

/// Whether `merchant_id` names a merchant owned by `auth`.
async fn merchant_is_owned(state: &AppState, auth: &AuthUser, merchant_id: &str) -> bool {
    let merchants = state.merchants.read().await;
    merchants
        .get(merchant_id)
        .map(|merchant| merchant.owner_user_id == auth.user_id)
        .unwrap_or(false)
}

/// Fetch the invoice `id` and check that it belongs to one of the
/// caller's merchants.
///
/// A missing invoice is `404`. An invoice whose merchant belongs to
/// another user is also reported as `404`, so invoice ids cannot be
/// used to probe other merchants' billing.
async fn load_owned_invoice(
    state: &AppState,
    auth: &AuthUser,
    id: &str,
) -> Result<Invoice, ApiError> {
    let invoice = {
        let invoices = state.invoices.read().await;
        invoices
            .get(id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("invoice not found: {id}")))?
    };
    if !merchant_is_owned(state, auth, &invoice.merchant_id).await {
        return Err(ApiError::NotFound(format!("invoice not found: {id}")));
    }
    Ok(invoice)
}

/// Short, human-facing suffix for an invoice number: the last eight
/// characters of the generated id, upper-cased.
fn short_suffix(id: &str) -> String {
    let tail: String = id
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    tail.to_ascii_uppercase()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /invoices` — create a draft invoice for an owned merchant.
///
/// The invoice number is derived from the generated id
/// (`INV-<SHORT-SUFFIX>`), the status starts as
/// [`InvoiceStatus::Draft`], and the response is `201 Created` with the
/// invoice and its computed total.
pub async fn create_invoice(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateInvoiceRequest>,
) -> Result<Response, ApiError> {
    ensure_merchant_owned(&state, &auth, &req.merchant_id).await?;

    if req.customer_name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "customer_name must not be empty".to_string(),
        ));
    }
    let customer_email = req.customer_email.trim();
    if customer_email.is_empty() || !customer_email.contains('@') {
        return Err(ApiError::BadRequest(
            "customer_email must be a valid email address".to_string(),
        ));
    }
    if req.items.is_empty() {
        return Err(ApiError::BadRequest(
            "items must contain at least one line item".to_string(),
        ));
    }
    if req.discount_minor < 0 {
        return Err(ApiError::BadRequest(
            "discount_minor must not be negative".to_string(),
        ));
    }
    let currency = Currency::from_code(&req.currency).ok_or_else(|| {
        ApiError::BadRequest(format!("unsupported currency: {}", req.currency))
    })?;

    let now = Utc::now();
    let id = invoice_id();
    let number = format!("INV-{}", short_suffix(&id));
    let invoice = Invoice {
        id: id.clone(),
        merchant_id: req.merchant_id.clone(),
        number,
        customer_name: req.customer_name.trim().to_string(),
        customer_email: customer_email.to_string(),
        items: req.items,
        currency,
        tax_bps: req.tax_bps,
        discount_minor: req.discount_minor,
        status: InvoiceStatus::Draft,
        due_at: req.due_days.map(|days| now + Duration::days(days)),
        paid_transaction_id: None,
        created_at: now,
    };

    let view = InvoiceView::from(&invoice);
    state
        .invoices
        .write()
        .await
        .insert(invoice.id.clone(), invoice);

    Ok((StatusCode::CREATED, Json(view)).into_response())
}

/// `GET /invoices` — list the caller's invoices.
///
/// Only invoices belonging to merchants owned by the caller are ever
/// returned. The optional `merchant_id` and `status` query parameters
/// narrow the result further.
pub async fn list_invoices(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListInvoicesQuery>,
) -> Result<Response, ApiError> {
    let status_filter = match query.status.as_deref() {
        Some(raw) => Some(parse_status(raw)?),
        None => None,
    };
    if let Some(merchant_id) = query.merchant_id.as_deref() {
        ensure_merchant_owned(&state, &auth, merchant_id).await?;
    }

    // Snapshot the caller's merchant ids first, so no lock is held
    // while the invoice map is scanned.
    let owned_merchant_ids: Vec<String> = {
        let merchants = state.merchants.read().await;
        merchants
            .values()
            .filter(|merchant| merchant.owner_user_id == auth.user_id)
            .map(|merchant| merchant.id.clone())
            .collect()
    };

    let invoices = state.invoices.read().await;
    let mut views: Vec<InvoiceView> = invoices
        .values()
        .filter(|invoice| owned_merchant_ids.contains(&invoice.merchant_id))
        .filter(|invoice| {
            query
                .merchant_id
                .as_deref()
                .map(|merchant_id| invoice.merchant_id == merchant_id)
                .unwrap_or(true)
        })
        .filter(|invoice| {
            status_filter
                .map(|status| invoice.status == status)
                .unwrap_or(true)
        })
        .map(InvoiceView::from)
        .collect();
    // Newest first; the id tie-break keeps the order deterministic.
    views.sort_by(|a, b| {
        b.invoice
            .created_at
            .cmp(&a.invoice.created_at)
            .then_with(|| a.invoice.id.cmp(&b.invoice.id))
    });

    Ok(Json(views).into_response())
}

/// `GET /invoices/{id}` — fetch one of the caller's invoices.
pub async fn get_invoice(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let invoice = load_owned_invoice(&state, &auth, &id).await?;
    Ok(Json(InvoiceView::from(&invoice)).into_response())
}

/// `POST /invoices/{id}/send` — issue a draft invoice to its customer.
///
/// Only a [`InvoiceStatus::Draft`] invoice can be sent; sending one in
/// any other state is a `409 Conflict`. On success the invoice moves to
/// [`InvoiceStatus::Sent`] and an email [`Notification`] built with
/// [`invoice_sent_body`] is queued for the customer.
pub async fn send_invoice(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    // Check ownership before mutating anything.
    load_owned_invoice(&state, &auth, &id).await?;

    let sent = {
        let mut invoices = state.invoices.write().await;
        let invoice = invoices
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("invoice not found: {id}")))?;
        if invoice.status != InvoiceStatus::Draft {
            return Err(ApiError::Conflict(format!(
                "invoice {} cannot be sent from status {:?}",
                invoice.number, invoice.status
            )));
        }
        invoice.status = InvoiceStatus::Sent;
        invoice.clone()
    };

    let notification = Notification::new(
        new_id("ntf"),
        Some(sent.merchant_id.clone()),
        NotificationChannel::Email,
        sent.customer_email.clone(),
        Some(format!("Invoice {}", sent.number)),
        invoice_sent_body(&sent.number, &sent.total().format_display()),
    );
    state.notifications.write().await.push(notification);

    Ok(Json(InvoiceView::from(&sent)).into_response())
}

/// `POST /invoices/{id}/pay` — pay an invoice. **Public**: no
/// authentication is required, because the payer is the customer, who
/// has no platform account.
///
/// Only a [`InvoiceStatus::Sent`] or [`InvoiceStatus::Overdue`] invoice
/// can be paid; anything else is a `409 Conflict`. A transaction for
/// the invoice total is created in
/// [`TxStatus::Processing`], the invoice is marked
/// [`InvoiceStatus::Paid`] with `paid_transaction_id` pointing at that
/// transaction, and the transaction id is returned.
pub async fn pay_invoice(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PayInvoiceRequest>,
) -> Result<Response, ApiError> {
    if req.gateway.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "gateway must not be empty".to_string(),
        ));
    }

    let invoice = {
        let invoices = state.invoices.read().await;
        invoices
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("invoice not found: {id}")))?
    };
    if !matches!(invoice.status, InvoiceStatus::Sent | InvoiceStatus::Overdue) {
        return Err(ApiError::Conflict(format!(
            "invoice {} cannot be paid from status {:?}",
            invoice.number, invoice.status
        )));
    }

    let now = Utc::now();
    let transaction = Transaction {
        id: transaction_id(),
        merchant_id: invoice.merchant_id.clone(),
        gateway: req.gateway.trim().to_string(),
        amount: invoice.total(),
        status: TxStatus::Processing,
        customer_email: Some(invoice.customer_email.clone()),
        customer_phone: None,
        provider_trx_id: None,
        sender_number: req.sender_number.clone(),
        reference: Some(invoice.number.clone()),
        metadata: serde_json::json!({
            "invoice_id": invoice.id,
            "invoice_number": invoice.number,
        }),
        created_at: now,
        updated_at: now,
    };
    let transaction_id = transaction.id.clone();
    state
        .transactions
        .write()
        .await
        .insert(transaction.id.clone(), transaction);

    // Re-check under the write lock: the invoice may have been paid or
    // cancelled by a concurrent request since the read above. If it is
    // no longer payable, the transaction just created stays on record
    // for reconciliation and the caller still gets the conflict.
    {
        let mut invoices = state.invoices.write().await;
        let stored = invoices
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("invoice not found: {id}")))?;
        if !matches!(stored.status, InvoiceStatus::Sent | InvoiceStatus::Overdue) {
            return Err(ApiError::Conflict(format!(
                "invoice {} cannot be paid from status {:?}",
                stored.number, stored.status
            )));
        }
        stored.status = InvoiceStatus::Paid;
        stored.paid_transaction_id = Some(transaction_id.clone());
    }

    Ok(Json(PayInvoiceResponse {
        transaction_id,
        invoice_id: id,
    })
    .into_response())
}

/// `POST /invoices/{id}/cancel` — cancel one of the caller's invoices.
///
/// A [`InvoiceStatus::Paid`] invoice can never be cancelled (`409
/// Conflict`); it must be refunded instead. Any other invoice moves to
/// [`InvoiceStatus::Cancelled`].
pub async fn cancel_invoice(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    // Check ownership before mutating anything.
    load_owned_invoice(&state, &auth, &id).await?;

    let cancelled = {
        let mut invoices = state.invoices.write().await;
        let invoice = invoices
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("invoice not found: {id}")))?;
        if invoice.status == InvoiceStatus::Paid {
            return Err(ApiError::Conflict(format!(
                "invoice {} has already been paid and cannot be cancelled",
                invoice.number
            )));
        }
        invoice.status = InvoiceStatus::Cancelled;
        invoice.clone()
    };

    Ok(Json(InvoiceView::from(&cancelled)).into_response())
}
