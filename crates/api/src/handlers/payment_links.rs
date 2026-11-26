//! Payment-link handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation. A payment link is a
//! merchant-owned, slug-addressed page a customer can open to pay.
//! The authenticated handlers manage links for merchants the caller
//! owns; the two `public_*` / `pay_*` handlers are unauthenticated
//! and are addressed by the link's public slug, never by its id.
//!
//! Routes served here:
//!
//! * `POST   /payment-links`               — [`create_link`]
//! * `GET    /payment-links`               — [`list_links`]
//! * `GET    /payment-links/{id}`          — [`get_link`]
//! * `POST   /payment-links/{id}/deactivate` — [`deactivate_link`]
//! * `GET    /pay/{slug}`                  — [`public_link`]
//! * `POST   /pay/{slug}`                  — [`pay_link`]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use miraclepay_domain::errors::DomainError;
use miraclepay_domain::ids::{payment_link_id, transaction_id};
use miraclepay_domain::money::{Currency, Money};
use miraclepay_domain::payment_link::{slugify, PaymentLink};
use miraclepay_domain::transaction::{Transaction, TxStatus};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / response bodies
// ---------------------------------------------------------------------------

/// Body of `POST /payment-links`.
#[derive(Debug, Deserialize)]
pub struct CreateLinkRequest {
    /// Merchant that will own the new link. Must belong to the caller.
    pub merchant_id: String,
    /// Human-readable title shown to the customer; also slugified
    /// into the link's public slug.
    pub title: String,
    /// Optional longer description shown to the customer.
    pub description: Option<String>,
    /// Fixed amount, in minor units. `None` makes the link
    /// open-amount: the customer chooses the amount when paying.
    pub amount_minor: Option<i64>,
    /// ISO 4217 currency code for `amount_minor`, e.g. `"BDT"`.
    pub currency: String,
    /// Whether checkout must collect the customer's phone number.
    pub collect_phone: bool,
    /// Whether checkout must collect the customer's email address.
    pub collect_email: bool,
    /// Optional cap on the number of times the link may be used.
    /// `Some(1)` makes the link one-time (Wave 15). `Some(0)` is
    /// rejected: a link that can never be used is a mistake.
    pub max_uses: Option<u32>,
    /// Wave 15: absolute expiry for the link (RFC 3339). Must be in
    /// the future. Takes precedence over `expires_in_hours`.
    pub expires_at: Option<chrono::DateTime<Utc>>,
    /// Wave 15: relative expiry — hours from creation. Must be > 0.
    pub expires_in_hours: Option<i64>,
}

/// Resolve a link's expiry from the create request (Wave 15).
///
/// Explicit `expires_at` wins over `expires_in_hours`; both must
/// describe a future moment or the request is rejected.
fn resolve_expiry(
    req: &CreateLinkRequest,
    now: chrono::DateTime<Utc>,
) -> Result<Option<chrono::DateTime<Utc>>, ApiError> {
    if let Some(at) = req.expires_at {
        if at <= now {
            return Err(ApiError::BadRequest(
                "expires_at must be in the future".to_string(),
            ));
        }
        return Ok(Some(at));
    }
    if let Some(hours) = req.expires_in_hours {
        if hours <= 0 {
            return Err(ApiError::BadRequest(
                "expires_in_hours must be greater than zero".to_string(),
            ));
        }
        return Ok(Some(now + chrono::Duration::hours(hours)));
    }
    Ok(None)
}

/// Query of `GET /payment-links`.
#[derive(Debug, Deserialize)]
pub struct ListLinksQuery {
    /// Restrict the listing to a single merchant, when given.
    pub merchant_id: Option<String>,
    /// Wave 11: optional 1-based page number (see `pagination`).
    pub page: Option<u32>,
    /// Wave 11: optional page size (clamped to 1..=100).
    pub per_page: Option<u32>,
}

/// Public, unauthenticated view of a usable payment link.
///
/// Deliberately omits internal fields (id, use counts, caps,
/// timestamps): a customer only needs to know what they are paying
/// for, how much, to whom, and what contact details are collected.
#[derive(Debug, Serialize)]
pub struct PublicLinkView {
    /// Title shown to the customer.
    pub title: String,
    /// Optional description shown to the customer.
    pub description: Option<String>,
    /// Fixed amount, or `None` for an open-amount link.
    pub amount: Option<Money>,
    /// Trading name of the merchant being paid.
    pub merchant_name: String,
    /// Whether checkout must collect the customer's phone number.
    pub collect_phone: bool,
    /// Whether checkout must collect the customer's email address.
    pub collect_email: bool,
    /// Per-merchant checkout branding (Wave 37).
    pub branding: PublicBranding,
}

/// The branding slice of [`PublicLinkView`]: everything optional —
/// an unbranded merchant gets all `None` and the platform default
/// look.
#[derive(Debug, Clone, Serialize)]
pub struct PublicBranding {
    pub display_name: Option<String>,
    pub brand_color: Option<String>,
    pub logo_url: Option<String>,
    pub checkout_message: Option<String>,
}

/// Body of `POST /pay/{slug}`.
#[derive(Debug, Deserialize)]
pub struct PayLinkRequest {
    /// Amount in minor units. Required for open-amount links;
    /// ignored when the link carries a fixed amount.
    pub amount_minor: Option<i64>,
    /// Customer's phone number, when collected / supplied.
    pub customer_phone: Option<String>,
    /// Customer's email address, when collected / supplied.
    pub customer_email: Option<String>,
    /// Gateway the customer chose to pay through, e.g. `"bkash"`.
    pub gateway: String,
}

/// Response of `POST /pay/{slug}`: the pending transaction that the
/// payment attempt created.
#[derive(Debug, Serialize)]
pub struct PayLinkResponse {
    /// Identifier of the newly created transaction.
    pub transaction_id: String,
    /// Status of that transaction (always `pending` at creation).
    pub status: TxStatus,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a poisoned-lock failure onto an internal API error.
fn lock_error() -> ApiError {
    ApiError::Internal("internal state lock poisoned".to_string())
}

/// Fail with `Forbidden` unless `merchant_id` names a merchant owned
/// by `auth`; `NotFound` when no such merchant exists at all.
fn ensure_merchant_owned(
    state: &AppState,
    auth: &AuthUser,
    merchant_id: &str,
) -> Result<(), ApiError> {
    let merchants = state.merchants.try_read().map_err(|_| lock_error())?;
    let merchant = merchants
        .get(merchant_id)
        .ok_or_else(|| ApiError::NotFound(format!("merchant {merchant_id} not found")))?;
    if merchant.owner_user_id != auth.user_id {
        return Err(ApiError::Forbidden(
            "merchant belongs to another user".to_string(),
        ));
    }
    Ok(())
}

/// Whether `merchant_id` names a merchant owned by `auth`
/// (a merchant missing from the map is treated as not owned).
fn merchant_owned(state: &AppState, auth: &AuthUser, merchant_id: &str) -> bool {
    match state.merchants.try_read() {
        Ok(merchants) => merchants
            .get(merchant_id)
            .map(|merchant| merchant.owner_user_id == auth.user_id)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Trading name of a merchant, or an empty string when the merchant
/// record is gone (a public page should not fail over that).
fn merchant_name(state: &AppState, merchant_id: &str) -> String {
    match state.merchants.try_read() {
        Ok(merchants) => merchants
            .get(merchant_id)
            .map(|merchant| merchant.business_name.clone())
            .unwrap_or_default(),
        Err(_) => String::new(),
    }
}

/// Translate a [`DomainError`] raised while using a link into the
/// API error that best describes it to the caller.
fn map_link_use_error(error: DomainError) -> ApiError {
    match error {
        DomainError::LinkInactive => ApiError::Conflict("payment link is inactive".to_string()),
        DomainError::LinkExhausted => {
            ApiError::Conflict("payment link has reached its maximum number of uses".to_string())
        }
        other => ApiError::BadRequest(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Authenticated handlers
// ---------------------------------------------------------------------------

/// Create a payment link for a merchant the caller owns.
///
/// The public slug is the slugified title plus a hyphen and the last
/// six characters of the freshly minted link id, which keeps slugs
/// readable while making collisions between same-titled links
/// unlikely; a slug that is still taken is a `409 Conflict`.
///
/// Returns `201 Created` with the stored link.
pub async fn create_link(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateLinkRequest>,
) -> Result<(StatusCode, Json<PaymentLink>), ApiError> {
    if req.title.trim().is_empty() {
        return Err(ApiError::BadRequest("title must not be empty".to_string()));
    }
    if req.max_uses == Some(0) {
        return Err(ApiError::BadRequest(
            "max_uses must be at least 1 when set".to_string(),
        ));
    }
    let expires_at = resolve_expiry(&req, Utc::now())?;
    ensure_merchant_owned(&state, &auth, &req.merchant_id)?;

    // A fixed amount needs a supported currency; an open-amount link
    // carries no amount at all.
    let amount = match req.amount_minor {
        Some(amount_minor) => {
            if amount_minor <= 0 {
                return Err(ApiError::BadRequest(
                    "amount_minor must be greater than zero".to_string(),
                ));
            }
            let currency = Currency::from_code(&req.currency).ok_or_else(|| {
                ApiError::BadRequest(format!("unsupported currency: {}", req.currency))
            })?;
            Some(Money::new(amount_minor, currency))
        }
        None => None,
    };

    let id = payment_link_id();
    let base = slugify(&req.title);
    let base = if base.is_empty() {
        "link".to_string()
    } else {
        base
    };
    // The id is long, ASCII and well over six characters, so taking
    // its last six bytes is always on a char boundary.
    let suffix = &id[id.len().saturating_sub(6)..];
    let slug = format!("{base}-{suffix}");

    let link = PaymentLink {
        id: id.clone(),
        merchant_id: req.merchant_id.clone(),
        slug: slug.clone(),
        title: req.title.clone(),
        description: req.description.clone(),
        amount,
        collect_phone: req.collect_phone,
        collect_email: req.collect_email,
        max_uses: req.max_uses,
        uses: 0,
        active: true,
        expires_at,
        created_at: Utc::now(),
    };

    {
        let mut links = state.payment_links.write().await;
        if links.values().any(|existing| existing.slug == slug) {
            return Err(ApiError::Conflict(format!(
                "payment link slug already exists: {slug}"
            )));
        }
        links.insert(id, link.clone());
    }

    Ok((StatusCode::CREATED, Json(link)))
}

/// List payment links belonging to merchants the caller owns,
/// optionally restricted to one merchant.
pub async fn list_links(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListLinksQuery>,
) -> Result<Json<Vec<PaymentLink>>, ApiError> {
    let links = state.payment_links.read().await;
    let mut out: Vec<PaymentLink> = links
        .values()
        .filter(|link| match &query.merchant_id {
            Some(merchant_id) => &link.merchant_id == merchant_id,
            None => true,
        })
        .filter(|link| merchant_owned(&state, &auth, &link.merchant_id))
        .cloned()
        .collect();
    out.sort_by_key(|link| link.created_at);
    let out = crate::pagination::apply_page(out, query.page, query.per_page);
    Ok(Json(out))
}

/// Fetch one payment link by id. The link's merchant must belong to
/// the caller.
pub async fn get_link(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PaymentLink>, ApiError> {
    let link = {
        let links = state.payment_links.read().await;
        links
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("payment link {id} not found")))?
    };
    ensure_merchant_owned(&state, &auth, &link.merchant_id)?;
    Ok(Json(link))
}

/// Deactivate a payment link: it stops being usable by customers
/// immediately, but the record (and its use count) is kept.
pub async fn deactivate_link(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PaymentLink>, ApiError> {
    let merchant_id = {
        let links = state.payment_links.read().await;
        links
            .get(&id)
            .map(|link| link.merchant_id.clone())
            .ok_or_else(|| ApiError::NotFound(format!("payment link {id} not found")))?
    };
    ensure_merchant_owned(&state, &auth, &merchant_id)?;

    let mut links = state.payment_links.write().await;
    let link = links
        .get_mut(&id)
        .ok_or_else(|| ApiError::NotFound(format!("payment link {id} not found")))?;
    link.active = false;
    Ok(Json(link.clone()))
}

// ---------------------------------------------------------------------------
// Public handlers (no authentication; addressed by slug)
// ---------------------------------------------------------------------------

/// Public view of a payment link, addressed by its slug.
///
/// A link that does not exist — or exists but is inactive, expired
/// or exhausted — is reported the same way, as
/// `404 NotFound("link not available")`, so the public endpoint does
/// not reveal which slugs exist but are switched off.
pub async fn public_link(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<PublicLinkView>, ApiError> {
    let link = {
        let links = state.payment_links.read().await;
        links.values().find(|link| link.slug == slug).cloned()
    };
    let link = match link {
        Some(link) if link.is_usable(Utc::now()) => link,
        _ => return Err(ApiError::NotFound("link not available".to_string())),
    };

    let branding = {
        let all_settings = state.merchant_settings.read().await;
        match all_settings.get(&link.merchant_id) {
            Some(settings) => PublicBranding {
                display_name: settings.brand_name.clone(),
                brand_color: settings.brand_color.clone(),
                logo_url: settings.logo_url.clone(),
                checkout_message: settings.checkout_message.clone(),
            },
            None => PublicBranding {
                display_name: None,
                brand_color: None,
                logo_url: None,
                checkout_message: None,
            },
        }
    };

    Ok(Json(PublicLinkView {
        title: link.title.clone(),
        description: link.description.clone(),
        amount: link.amount,
        merchant_name: merchant_name(&state, &link.merchant_id),
        collect_phone: link.collect_phone,
        collect_email: link.collect_email,
        branding,
    }))
}

/// Pay through a payment link, addressed by its slug.
///
/// The amount is the link's fixed amount when it has one, otherwise
/// the amount the customer supplied in the request; having neither
/// is a `400 BadRequest`. One use of the link is recorded (domain
/// errors from [`PaymentLink::record_use`] are mapped onto API
/// errors), and a `Pending` transaction referencing the slug is
/// stored. Returns `201 Created` with the transaction id and status.
pub async fn pay_link(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<PayLinkRequest>,
) -> Result<(StatusCode, Json<PayLinkResponse>), ApiError> {
    if req.gateway.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "gateway must not be empty".to_string(),
        ));
    }
    let now = Utc::now();

    // Resolve everything the transaction needs while holding the
    // write lock, record the use, then release the lock before
    // touching the transactions map.
    let (merchant_id, amount) = {
        let mut links = state.payment_links.write().await;
        let link = links
            .values_mut()
            .find(|link| link.slug == slug)
            .ok_or_else(|| ApiError::NotFound("link not available".to_string()))?;
        if !link.is_usable(now) {
            return Err(ApiError::NotFound("link not available".to_string()));
        }

        let amount = match link.amount {
            Some(fixed) => fixed,
            None => match req.amount_minor {
                Some(amount_minor) if amount_minor > 0 => {
                    // An open-amount link carries no currency; the
                    // platform's primary currency (BDT) applies.
                    Money::new(amount_minor, Currency::BDT)
                }
                _ => {
                    return Err(ApiError::BadRequest(
                        "amount_minor is required for an open-amount link".to_string(),
                    ));
                }
            },
        };

        link.record_use().map_err(map_link_use_error)?;
        (link.merchant_id.clone(), amount)
    };

    let id = transaction_id();
    let transaction = Transaction {
        id: id.clone(),
        merchant_id,
        gateway: req.gateway.clone(),
        amount,
        status: TxStatus::Pending,
        customer_email: req.customer_email.clone(),
        customer_phone: req.customer_phone.clone(),
        provider_trx_id: None,
        sender_number: None,
        reference: Some(slug.clone()),
        metadata: serde_json::json!({ "payment_link_slug": slug }),
        created_at: now,
        updated_at: now,
    };

    {
        let mut transactions = state.transactions.write().await;
        transactions.insert(id.clone(), transaction.clone());
    }
    state.assess_risk(&transaction).await;

    Ok((
        StatusCode::CREATED,
        Json(PayLinkResponse {
            transaction_id: id,
            status: TxStatus::Pending,
        }),
    ))
}

#[cfg(test)]
mod expiry_tests {
    use super::*;
    use chrono::TimeZone;

    fn req(expires_at: Option<chrono::DateTime<Utc>>, hours: Option<i64>) -> CreateLinkRequest {
        CreateLinkRequest {
            merchant_id: "m-1".to_string(),
            title: "Test".to_string(),
            description: None,
            amount_minor: None,
            currency: "BDT".to_string(),
            collect_phone: false,
            collect_email: false,
            max_uses: None,
            expires_at,
            expires_in_hours: hours,
        }
    }

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 26, 4, 0, 0).unwrap()
    }

    #[test]
    fn no_expiry_fields_means_no_expiry() {
        assert_eq!(resolve_expiry(&req(None, None), now()).unwrap(), None);
    }

    #[test]
    fn explicit_future_expiry_wins_over_hours() {
        let at = now() + chrono::Duration::days(2);
        assert_eq!(
            resolve_expiry(&req(Some(at), Some(1)), now()).unwrap(),
            Some(at)
        );
    }

    #[test]
    fn past_expiry_is_rejected() {
        let at = now() - chrono::Duration::hours(1);
        assert!(resolve_expiry(&req(Some(at), None), now()).is_err());
    }

    #[test]
    fn relative_hours_resolve_from_now() {
        let got = resolve_expiry(&req(None, Some(48)), now()).unwrap();
        assert_eq!(got, Some(now() + chrono::Duration::hours(48)));
        assert!(resolve_expiry(&req(None, Some(0)), now()).is_err());
        assert!(resolve_expiry(&req(None, Some(-5)), now()).is_err());
    }
}
