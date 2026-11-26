//! Merchant HTTP handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation built on Axum 0.7.
//!
//! These handlers manage the merchant (business) records owned by the
//! authenticated user:
//!
//! * `POST   /merchants`                — [`create_merchant`]
//! * `GET    /merchants`                — [`list_my_merchants`]
//! * `GET    /merchants/{id}`           — [`get_merchant`]
//! * `PATCH  /merchants/{id}`           — [`update_merchant`]
//! * `GET    /merchants/{id}/balance`   — [`merchant_balance`]
//!
//! Ownership rules are enforced here, in the handler layer:
//!
//! * A merchant is always created for the authenticated user; any
//!   `owner_user_id` supplied in the request body is discarded.
//! * Listing only ever returns the caller's own merchants.
//! * Reading a merchant or its balance requires being its owner or a
//!   platform admin. Anyone else gets `404 Not Found` rather than
//!   `403 Forbidden`, so the existence of other users' merchants is
//!   not leaked.
//! * Updating a merchant is owner-only (admins who do not own the
//!   merchant cannot edit it) and likewise answers `404` for
//!   non-owners.
//!
//! All state lives in the shared [`AppState`](crate::state::AppState)
//! in-memory maps; the handlers below are the only writers of the
//! `merchants` map in this module.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use miraclepay_domain::{
    ids::merchant_id,
    merchant::{validate_create, CreateMerchant, Merchant, MerchantStatus},
    money::Currency,
    settings,
    stats::{compute_balance, MerchantBalance},
};

use crate::{
    auth_middleware::AuthUser, dto::ApiError, state::AppState,
};

/// Request body for [`update_merchant`].
///
/// Every field is optional: a field left out (or sent as `null`) is
/// left unchanged on the stored merchant, while a supplied field
/// replaces the stored value. There is deliberately no way to change
/// `id`, `owner_user_id`, `email`, `country`, `status`, or
/// `created_at` through this endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct UpdateReq {
    /// New legal / trading name of the business.
    pub business_name: Option<String>,
    /// New contact phone number.
    pub phone: Option<String>,
    /// New URL notified about payment events for this merchant.
    pub webhook_url: Option<String>,
}

/// Whether `auth` may view the merchant owned by `owner_user_id`:
/// the owner themselves, or any platform admin.
fn can_view(auth: &AuthUser, owner_user_id: &str) -> bool {
    auth.user_id == owner_user_id || auth.is_admin()
}

/// `POST /merchants` — create a merchant owned by the caller.
///
/// The owner is forced to the authenticated user, whatever the body
/// claims. The input is validated with the domain rules
/// ([`validate_create`]); every validation problem is reported at
/// once as a single `400 Bad Request` message. New merchants start
/// out [`MerchantStatus::Active`].
///
/// Responds `201 Created` with the stored merchant as JSON.
pub async fn create_merchant(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(mut input): Json<CreateMerchant>,
) -> Result<(StatusCode, Json<Merchant>), ApiError> {
    // Ownership comes from the authenticated session, never the body.
    input.owner_user_id = auth.user_id.clone();

    validate_create(&input)
        .map_err(|errors| ApiError::BadRequest(errors.join("; ")))?;

    let merchant = Merchant {
        id: merchant_id(),
        owner_user_id: input.owner_user_id,
        business_name: input.business_name,
        email: input.email,
        phone: input.phone,
        country: input.country,
        status: MerchantStatus::Active,
        webhook_url: input.webhook_url,
        created_at: Utc::now(),
    };

    state
        .merchants
        .write()
        .await
        .insert(merchant.id.clone(), merchant.clone());

    Ok((StatusCode::CREATED, Json(merchant)))
}

/// `GET /merchants` — list the merchants owned by the caller.
///
/// Merchants belonging to anyone else are never included, including
/// for admins: this is the "my merchants" view. Results are ordered
/// by creation time so the response is stable between calls.
pub async fn list_my_merchants(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Merchant>>, ApiError> {
    let merchants = state.merchants.read().await;

    let mut mine: Vec<Merchant> = merchants
        .values()
        .filter(|merchant| merchant.owner_user_id == auth.user_id)
        .cloned()
        .collect();
    mine.sort_by_key(|merchant| merchant.created_at);

    Ok(Json(mine))
}

/// `GET /merchants/{id}` — fetch one merchant by id.
///
/// Only the owner or a platform admin may read a merchant; for
/// everyone else the merchant is reported as not found.
pub async fn get_merchant(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Merchant>, ApiError> {
    let merchants = state.merchants.read().await;

    let merchant = merchants
        .get(&id)
        .filter(|merchant| can_view(&auth, &merchant.owner_user_id))
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("merchant {id} not found")))?;

    Ok(Json(merchant))
}

/// `PATCH /merchants/{id}` — update a merchant's editable fields.
///
/// Owner-only: a caller who does not own the merchant (including an
/// admin) gets `404 Not Found`, matching [`get_merchant`]. Only the
/// fields present in the body are changed. A supplied
/// `business_name` must be non-empty and a supplied `webhook_url`
/// must be an `http://` or `https://` URL, mirroring the domain
/// creation rules; violations are `400 Bad Request` and leave the
/// stored merchant untouched.
///
/// Responds with the updated merchant as JSON.
pub async fn update_merchant(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(update): Json<UpdateReq>,
) -> Result<Json<Merchant>, ApiError> {
    // Validate the supplied fields before touching stored state, so
    // a rejected update is never partially applied.
    if let Some(name) = update.business_name.as_deref() {
        if name.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "business_name must not be empty".to_string(),
            ));
        }
    }
    if let Some(url) = update.webhook_url.as_deref() {
        let url = url.trim();
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ApiError::BadRequest(
                "webhook_url must start with http:// or https://".to_string(),
            ));
        }
    }

    let mut merchants = state.merchants.write().await;

    let merchant = merchants
        .get_mut(&id)
        .filter(|merchant| merchant.owner_user_id == auth.user_id)
        .ok_or_else(|| ApiError::NotFound(format!("merchant {id} not found")))?;

    if let Some(business_name) = update.business_name {
        merchant.business_name = business_name;
    }
    if let Some(phone) = update.phone {
        merchant.phone = Some(phone);
    }
    if let Some(webhook_url) = update.webhook_url {
        merchant.webhook_url = Some(webhook_url);
    }

    Ok(Json(merchant.clone()))
}

/// `GET /merchants/{id}/balance` — the merchant's balance in BDT.
///
/// Access matches [`get_merchant`]: owner or platform admin, with
/// `404 Not Found` for everyone else (and for unknown ids). The
/// balance itself is pure domain arithmetic
/// ([`compute_balance`]): completed captures minus succeeded refunds
/// minus settlements already paid out, plus the still-pending total,
/// all in Bangladeshi taka — the platform's settlement currency.
pub async fn merchant_balance(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MerchantBalance>, ApiError> {
    // Existence + access check first, so an unauthorized caller
    // cannot distinguish "no such merchant" from "not yours", and
    // cannot learn a balance for a merchant that does not exist.
    {
        let merchants = state.merchants.read().await;
        merchants
            .get(&id)
            .filter(|merchant| can_view(&auth, &merchant.owner_user_id))
            .ok_or_else(|| ApiError::NotFound(format!("merchant {id} not found")))?;
    }

    // Snapshot the three ledgers, then let the domain do the maths.
    // `compute_balance` itself restricts every sum to this merchant
    // and to the requested currency.
    let transactions: Vec<_> = state
        .transactions
        .read()
        .await
        .values()
        .cloned()
        .collect();
    let refunds: Vec<_> = state.refunds.read().await.values().cloned().collect();
    let settlements: Vec<_> = state
        .settlements
        .read()
        .await
        .values()
        .cloned()
        .collect();

    let balance = compute_balance(
        &id,
        &transactions,
        &refunds,
        &settlements,
        Currency::BDT,
    );

    Ok(Json(balance))
}

// ---------------------------------------------------------------------------
// Merchant settings incl. checkout branding (Wave 37)
// ---------------------------------------------------------------------------

/// Default settings for a merchant that has never saved any:
/// platform fee applies, notifications on, no branding.
pub fn default_merchant_settings(merchant_id: &str) -> settings::MerchantSettings {
    settings::MerchantSettings {
        merchant_id: merchant_id.to_string(),
        fee_bps: None,
        auto_settle: false,
        settle_threshold_minor: 0,
        notify_on_payment: true,
        notify_on_refund: true,
        brand_name: None,
        brand_color: None,
        logo_url: None,
        checkout_message: None,
    }
}

/// `GET /merchants/:id/settings` — the merchant's settings
/// (defaults when never saved). Owner or admin.
pub async fn get_merchant_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<settings::MerchantSettings>, ApiError> {
    let merchants = state.merchants.read().await;
    merchants
        .get(&id)
        .filter(|merchant| can_view(&auth, &merchant.owner_user_id))
        .ok_or_else(|| ApiError::NotFound(format!("merchant {id} not found")))?;
    drop(merchants);
    let stored = state.merchant_settings.read().await;
    Ok(Json(
        stored
            .get(&id)
            .cloned()
            .unwrap_or_else(|| default_merchant_settings(&id)),
    ))
}

/// `PUT /merchants/:id/settings` — replace the merchant's
/// settings. Owner-only. Domain validation errors are returned
/// together as a 400.
pub async fn update_merchant_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut body): Json<settings::MerchantSettings>,
) -> Result<Json<settings::MerchantSettings>, ApiError> {
    {
        let merchants = state.merchants.read().await;
        let merchant = merchants
            .get(&id)
            .ok_or_else(|| ApiError::NotFound(format!("merchant {id} not found")))?;
        if merchant.owner_user_id != auth.user_id {
            return Err(ApiError::Forbidden(
                "only the merchant owner can change settings".to_string(),
            ));
        }
    }
    body.merchant_id = id.clone();
    settings::validate_merchant_settings(&body).map_err(|errors| {
        ApiError::BadRequest(format!("invalid merchant settings: {}", errors.join("; ")))
    })?;
    state
        .merchant_settings
        .write()
        .await
        .insert(id.clone(), body.clone());
    state.audit(
        Some(auth.user_id.clone()),
        Some(id.clone()),
        "merchant.settings_updated",
        "merchant",
        None,
    );
    Ok(Json(body))
}
