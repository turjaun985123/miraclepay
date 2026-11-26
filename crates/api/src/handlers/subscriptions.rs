//! Subscription plan & subscription handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation. A plan is a merchant-owned
//! recurring price point; a subscription binds one customer to one
//! plan. Every handler here is authenticated and enforces merchant
//! ownership exactly like the sibling handlers (see
//! `payment_links.rs`): a missing merchant is `404`, a merchant owned
//! by another user is `403`, and listings only ever contain records
//! belonging to the caller's merchants.
//!
//! Routes served here:
//!
//! * `POST   /subscriptions/plans`                  — [`create_plan`]
//! * `GET    /subscriptions/plans`                  — [`list_plans`]
//! * `GET    /subscriptions/plans/{id}`             — [`get_plan`]
//! * `PATCH  /subscriptions/plans/{id}`             — [`update_plan`]
//! * `POST   /subscriptions`                        — [`create_subscription`]
//! * `GET    /subscriptions`                        — [`list_subscriptions`]
//! * `POST   /subscriptions/{id}/cancel`            — [`cancel_subscription`]
//! * `POST   /subscriptions/{id}/pause`             — [`pause_subscription`]
//! * `POST   /subscriptions/{id}/resume`            — [`resume_subscription`]
//!
//! Domain-model notes (v0.0.1): the domain [`Plan`] stores a single
//! [`BillingInterval`] unit, so a plan bills once per interval unit;
//! the currency is carried by the plan's [`Money`] amount itself and
//! is never a separate field. The domain [`Subscription`] has no
//! customer-name field and no dedicated paused status: pausing is
//! expressed with the domain's scheduled-stop mechanism
//! ([`Subscription::cancel_at_period_end`]) — the subscription stays
//! usable until the current period ends and is not charged again —
//! and resuming clears it with [`Subscription::resume`]. Immediate
//! cancellation uses [`Subscription::cancel_now`].

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;

use miraclepay_domain::ids::new_id;
use miraclepay_domain::money::Money;
use miraclepay_domain::subscription::{BillingInterval, Plan, Subscription, SubscriptionStatus};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / query bodies
// ---------------------------------------------------------------------------

/// Body of `POST /subscriptions/plans`.
#[derive(Debug, Deserialize)]
pub struct CreatePlanRequest {
    /// Merchant that will own the new plan. Must belong to the caller.
    pub merchant_id: String,
    /// Human-readable plan name, e.g. `"Pro monthly"`.
    pub name: String,
    /// Amount charged once per interval. The currency is taken from
    /// this amount; there is no separate currency field.
    pub amount: Money,
    /// Billing interval unit: `daily`, `weekly`, `monthly`,
    /// `quarterly` or `yearly`.
    pub interval: BillingInterval,
    /// How many interval units make one billing period. v0.0.1 plans
    /// bill every single unit, so when given this must be `1`.
    pub interval_count: Option<u32>,
    /// Free trial length in days before the first charge; `0` (or
    /// omitted) means no trial.
    pub trial_days: Option<u32>,
}

/// Query of `GET /subscriptions/plans`.
#[derive(Debug, Deserialize)]
pub struct ListPlansQuery {
    /// Restrict the listing to a single merchant, when given.
    pub merchant_id: Option<String>,
}

/// Body of `PATCH /subscriptions/plans/{id}`. Every field is
/// optional; only the fields present are changed.
#[derive(Debug, Deserialize)]
pub struct UpdatePlanRequest {
    /// New plan name.
    pub name: Option<String>,
    /// New per-period amount (its currency replaces the old one).
    pub amount: Option<Money>,
    /// Whether the plan can be subscribed to. Existing subscriptions
    /// are unaffected either way.
    pub active: Option<bool>,
}

/// Query of `GET /subscriptions`.
#[derive(Debug, Deserialize)]
pub struct ListSubscriptionsQuery {
    /// Restrict the listing to a single merchant, when given.
    pub merchant_id: Option<String>,
    /// Restrict the listing to a single lifecycle status, spelled as
    /// in the domain type: `trialing`, `active`, `past_due`,
    /// `cancelled` or `expired`.
    pub status: Option<String>,
}

/// Body of `POST /subscriptions`.
#[derive(Debug, Deserialize)]
pub struct CreateSubscriptionRequest {
    /// Plan to subscribe to. Must be active and belong to a merchant
    /// owned by the caller.
    pub plan_id: String,
    /// Email address of the subscribing customer.
    pub customer_email: String,
    /// Display name of the subscribing customer. Accepted for API
    /// completeness; the v0.0.1 domain subscription record has no
    /// name field, so it is validated but not stored.
    pub customer_name: Option<String>,
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

/// Parse a `status` query value into a [`SubscriptionStatus`].
fn parse_status(raw: &str) -> Result<SubscriptionStatus, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "trialing" => Ok(SubscriptionStatus::Trialing),
        "active" => Ok(SubscriptionStatus::Active),
        "past_due" => Ok(SubscriptionStatus::PastDue),
        "cancelled" => Ok(SubscriptionStatus::Cancelled),
        "expired" => Ok(SubscriptionStatus::Expired),
        other => Err(ApiError::BadRequest(format!(
            "unknown subscription status: {other}"
        ))),
    }
}

/// Fetch the subscription `id` after checking that its merchant
/// belongs to the caller.
async fn load_owned_subscription(
    state: &AppState,
    auth: &AuthUser,
    id: &str,
) -> Result<Subscription, ApiError> {
    let subscription = {
        let subscriptions = state.subscriptions.read().await;
        subscriptions
            .get(id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("subscription {id} not found")))?
    };
    ensure_merchant_owned(state, auth, &subscription.merchant_id)?;
    Ok(subscription)
}

// ---------------------------------------------------------------------------
// Plan handlers
// ---------------------------------------------------------------------------

/// Create a subscription plan for a merchant the caller owns.
///
/// Returns `201 Created` with the stored plan.
pub async fn create_plan(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreatePlanRequest>,
) -> Result<(StatusCode, Json<Plan>), ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".to_string()));
    }
    if req.amount.amount_minor <= 0 {
        return Err(ApiError::BadRequest(
            "amount must be greater than zero".to_string(),
        ));
    }
    if let Some(count) = req.interval_count {
        if count != 1 {
            return Err(ApiError::BadRequest(
                "interval_count must be 1 in v0.0.1: plans bill once per interval unit".to_string(),
            ));
        }
    }
    ensure_merchant_owned(&state, &auth, &req.merchant_id)?;

    let id = new_id("pln");
    let plan = Plan::new(
        id.clone(),
        req.merchant_id.clone(),
        req.name.clone(),
        req.amount,
        req.interval,
        req.trial_days.unwrap_or(0),
        true,
    );

    {
        let mut plans = state.plans.write().await;
        plans.insert(id, plan.clone());
    }

    state.audit(
        Some(auth.user_id.clone()),
        Some(plan.merchant_id.clone()),
        "plan.created",
        "plan",
        Some(plan.id.clone()),
    );

    Ok((StatusCode::CREATED, Json(plan)))
}

/// List subscription plans belonging to merchants the caller owns,
/// optionally restricted to one merchant.
pub async fn list_plans(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListPlansQuery>,
) -> Result<Json<Vec<Plan>>, ApiError> {
    let plans = state.plans.read().await;
    let mut out: Vec<Plan> = plans
        .values()
        .filter(|plan| match &query.merchant_id {
            Some(merchant_id) => &plan.merchant_id == merchant_id,
            None => true,
        })
        .filter(|plan| merchant_owned(&state, &auth, &plan.merchant_id))
        .cloned()
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(Json(out))
}

/// Fetch one subscription plan by id. The plan's merchant must
/// belong to the caller.
pub async fn get_plan(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Plan>, ApiError> {
    let plan = {
        let plans = state.plans.read().await;
        plans
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("plan {id} not found")))?
    };
    ensure_merchant_owned(&state, &auth, &plan.merchant_id)?;
    Ok(Json(plan))
}

/// Update a subscription plan: name, amount and/or active flag.
/// Only the fields present in the body are changed.
pub async fn update_plan(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePlanRequest>,
) -> Result<Json<Plan>, ApiError> {
    if let Some(name) = &req.name {
        if name.trim().is_empty() {
            return Err(ApiError::BadRequest("name must not be empty".to_string()));
        }
    }
    if let Some(amount) = &req.amount {
        if amount.amount_minor <= 0 {
            return Err(ApiError::BadRequest(
                "amount must be greater than zero".to_string(),
            ));
        }
    }

    let merchant_id = {
        let plans = state.plans.read().await;
        plans
            .get(&id)
            .map(|plan| plan.merchant_id.clone())
            .ok_or_else(|| ApiError::NotFound(format!("plan {id} not found")))?
    };
    ensure_merchant_owned(&state, &auth, &merchant_id)?;

    let plan = {
        let mut plans = state.plans.write().await;
        let plan = plans
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("plan {id} not found")))?;
        if let Some(name) = req.name {
            plan.name = name;
        }
        if let Some(amount) = req.amount {
            plan.amount = amount;
        }
        if let Some(active) = req.active {
            plan.active = active;
        }
        plan.clone()
    };

    state.audit(
        Some(auth.user_id.clone()),
        Some(plan.merchant_id.clone()),
        "plan.updated",
        "plan",
        Some(plan.id.clone()),
    );

    Ok(Json(plan))
}

// ---------------------------------------------------------------------------
// Subscription handlers
// ---------------------------------------------------------------------------

/// List subscriptions belonging to merchants the caller owns,
/// optionally restricted to one merchant and/or one status.
pub async fn list_subscriptions(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListSubscriptionsQuery>,
) -> Result<Json<Vec<Subscription>>, ApiError> {
    let status_filter = match query.status.as_deref() {
        Some(raw) => Some(parse_status(raw)?),
        None => None,
    };

    let subscriptions = state.subscriptions.read().await;
    let mut out: Vec<Subscription> = subscriptions
        .values()
        .filter(|subscription| match &query.merchant_id {
            Some(merchant_id) => &subscription.merchant_id == merchant_id,
            None => true,
        })
        .filter(|subscription| {
            status_filter
                .map(|status| subscription.status == status)
                .unwrap_or(true)
        })
        .filter(|subscription| merchant_owned(&state, &auth, &subscription.merchant_id))
        .cloned()
        .collect();
    out.sort_by_key(|subscription| subscription.created_at);
    Ok(Json(out))
}

/// Subscribe a customer to a plan.
///
/// The plan must exist, be active, and belong to a merchant owned by
/// the caller. The new subscription starts immediately via
/// [`Subscription::start`]: `Trialing` until the trial ends when the
/// plan has one, otherwise `Active` for one interval. Returns
/// `201 Created` with the stored subscription.
pub async fn create_subscription(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateSubscriptionRequest>,
) -> Result<(StatusCode, Json<Subscription>), ApiError> {
    let customer_email = req.customer_email.trim();
    if customer_email.is_empty() || !customer_email.contains('@') {
        return Err(ApiError::BadRequest(
            "customer_email must be a valid email address".to_string(),
        ));
    }
    if let Some(name) = &req.customer_name {
        if name.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "customer_name must not be empty".to_string(),
            ));
        }
    }

    let plan = {
        let plans = state.plans.read().await;
        plans
            .get(&req.plan_id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("plan {} not found", req.plan_id)))?
    };
    ensure_merchant_owned(&state, &auth, &plan.merchant_id)?;
    if !plan.active {
        return Err(ApiError::Conflict(format!(
            "plan {} is inactive and cannot be subscribed to",
            plan.id
        )));
    }

    let subscription =
        Subscription::start(new_id("sub"), &plan, customer_email.to_string(), Utc::now());

    {
        let mut subscriptions = state.subscriptions.write().await;
        subscriptions.insert(subscription.id.clone(), subscription.clone());
    }

    state.audit(
        Some(auth.user_id.clone()),
        Some(subscription.merchant_id.clone()),
        "subscription.created",
        "subscription",
        Some(subscription.id.clone()),
    );

    // Wave 32: webhook event alongside the audit entry.
    state.emit_webhook(
        &subscription.merchant_id,
        "subscription.created",
        serde_json::json!({
            "subscription_id": subscription.id,
            "merchant_id": subscription.merchant_id,
            "plan_id": subscription.plan_id,
            "status": subscription.status,
            "customer_email": subscription.customer_email,
        }),
    );

    Ok((StatusCode::CREATED, Json(subscription)))
}

/// Cancel a subscription immediately: its status becomes
/// `Cancelled` (terminal) and it is never charged again. Cancelling
/// an already-terminal subscription is a `409 Conflict`.
pub async fn cancel_subscription(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Subscription>, ApiError> {
    // Check ownership before mutating anything.
    load_owned_subscription(&state, &auth, &id).await?;

    let subscription = {
        let mut subscriptions = state.subscriptions.write().await;
        let subscription = subscriptions
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("subscription {id} not found")))?;
        if subscription.status.is_terminal() {
            return Err(ApiError::Conflict(format!(
                "subscription {id} is already {:?} and cannot be cancelled",
                subscription.status
            )));
        }
        subscription.cancel_now();
        subscription.clone()
    };

    state.audit(
        Some(auth.user_id.clone()),
        Some(subscription.merchant_id.clone()),
        "subscription.cancelled",
        "subscription",
        Some(subscription.id.clone()),
    );

    // Wave 32: webhook event alongside the audit entry.
    state.emit_webhook(
        &subscription.merchant_id,
        "subscription.cancelled",
        serde_json::json!({
            "subscription_id": subscription.id,
            "merchant_id": subscription.merchant_id,
            "plan_id": subscription.plan_id,
            "status": subscription.status,
            "customer_email": subscription.customer_email,
        }),
    );

    Ok(Json(subscription))
}

/// Pause a subscription: it keeps working until the current period
/// ends and is not charged again, until it is resumed. (v0.0.1 has
/// no dedicated paused status; this uses the domain's scheduled
/// stop, [`Subscription::cancel_at_period_end`].) Pausing a terminal
/// subscription is a `409 Conflict`.
pub async fn pause_subscription(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Subscription>, ApiError> {
    // Check ownership before mutating anything.
    load_owned_subscription(&state, &auth, &id).await?;

    let subscription = {
        let mut subscriptions = state.subscriptions.write().await;
        let subscription = subscriptions
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("subscription {id} not found")))?;
        if subscription.status.is_terminal() {
            return Err(ApiError::Conflict(format!(
                "subscription {id} is {:?} and cannot be paused",
                subscription.status
            )));
        }
        subscription.cancel_at_period_end();
        subscription.clone()
    };

    state.audit(
        Some(auth.user_id.clone()),
        Some(subscription.merchant_id.clone()),
        "subscription.paused",
        "subscription",
        Some(subscription.id.clone()),
    );

    // Wave 32: webhook event alongside the audit entry.
    state.emit_webhook(
        &subscription.merchant_id,
        "subscription.paused",
        serde_json::json!({
            "subscription_id": subscription.id,
            "merchant_id": subscription.merchant_id,
            "plan_id": subscription.plan_id,
            "status": subscription.status,
            "customer_email": subscription.customer_email,
        }),
    );

    Ok(Json(subscription))
}

/// Resume a paused subscription: the scheduled stop is cleared via
/// [`Subscription::resume`] and normal renewal (and billing) is
/// restored. Resuming a terminal subscription is a `409 Conflict`,
/// since a cancelled or expired subscription cannot be reactivated
/// in place.
pub async fn resume_subscription(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Subscription>, ApiError> {
    // Check ownership before mutating anything.
    load_owned_subscription(&state, &auth, &id).await?;

    let subscription = {
        let mut subscriptions = state.subscriptions.write().await;
        let subscription = subscriptions
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("subscription {id} not found")))?;
        if subscription.status.is_terminal() {
            return Err(ApiError::Conflict(format!(
                "subscription {id} is {:?} and cannot be resumed",
                subscription.status
            )));
        }
        subscription.resume();
        subscription.clone()
    };

    state.audit(
        Some(auth.user_id.clone()),
        Some(subscription.merchant_id.clone()),
        "subscription.resumed",
        "subscription",
        Some(subscription.id.clone()),
    );

    // Wave 32: webhook event alongside the audit entry.
    state.emit_webhook(
        &subscription.merchant_id,
        "subscription.resumed",
        serde_json::json!({
            "subscription_id": subscription.id,
            "merchant_id": subscription.merchant_id,
            "plan_id": subscription.plan_id,
            "status": subscription.status,
            "customer_email": subscription.customer_email,
        }),
    );

    Ok(Json(subscription))
}
