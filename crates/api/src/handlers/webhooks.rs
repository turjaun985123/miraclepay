//! Webhook handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation (Axum 0.7).
//!
//! Endpoints (wired up by the router in `crate::routes`):
//!
//! - `GET  /webhooks/events`                 — [`list_events`]
//! - `GET  /webhooks/deliveries`             — [`list_deliveries`]
//! - `POST /webhooks/events/:event_id/redeliver` — [`redeliver`]
//! - `POST /webhooks/test-signature`         — [`test_signature`]
//!
//! Visibility rule shared by every handler in this module: a
//! non-admin caller only ever sees events (and their deliveries) that
//! belong to merchants they own; an admin sees everything.
//!
//! These handlers never perform outbound HTTP themselves. Creating a
//! delivery only records it as [`DeliveryStatus::Pending`]; the
//! background worker is responsible for picking pending deliveries up,
//! signing their payloads, POSTing them to the merchant's webhook URL,
//! and applying the domain retry/backoff policy.

use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use miraclepay_domain::{
    ids,
    webhook::{verify_signature, DeliveryStatus, WebhookDelivery, WebhookEvent},
};
use serde::{Deserialize, Serialize};

use crate::{auth_middleware::AuthUser, dto::ApiError, state::AppState};

// ---------------------------------------------------------------------------
// Query / body / response types
// ---------------------------------------------------------------------------

/// Query parameters for [`list_events`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListEventsQuery {
    /// Restrict the result to a single merchant, when given. The
    /// merchant must still be owned by the caller (unless the caller
    /// is an admin) — filtering by an unowned merchant yields an
    /// empty list, never that merchant's events.
    pub merchant_id: Option<String>,
    /// Wave 11: optional 1-based page number (see `pagination`).
    pub page: Option<u32>,
    /// Wave 11: optional page size (clamped to 1..=100).
    pub per_page: Option<u32>,
}

/// Query parameters for [`list_deliveries`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListDeliveriesQuery {
    /// Restrict the result to deliveries of a single event, when given.
    pub event_id: Option<String>,
    /// Wave 11: optional 1-based page number (see `pagination`).
    pub page: Option<u32>,
    /// Wave 11: optional page size (clamped to 1..=100).
    pub per_page: Option<u32>,
}

/// Request body for [`test_signature`].
#[derive(Debug, Clone, Deserialize)]
pub struct SigReq {
    /// Webhook signing secret the signature is claimed to be keyed by.
    pub secret: String,
    /// Raw payload bytes, as a UTF-8 string, exactly as they were signed.
    pub payload: String,
    /// Hex-encoded HMAC-SHA256 signature to check.
    pub signature: String,
}

/// Response body for [`test_signature`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SigResp {
    /// Whether `signature` is a valid signature of `payload` under `secret`.
    pub valid: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Whether `auth` belongs to a platform administrator.
fn is_admin(auth: &AuthUser) -> bool {
    auth.is_admin()
}

/// Ids of every merchant owned by the caller.
async fn owned_merchant_ids(state: &AppState, auth: &AuthUser) -> HashSet<String> {
    state
        .merchants
        .read()
        .await
        .values()
        .filter(|merchant| merchant.owner_user_id == auth.user_id)
        .map(|merchant| merchant.id.clone())
        .collect()
}

/// Whether the caller may see content belonging to `merchant_id`:
/// admins may see everything, everyone else only their own merchants.
fn may_see(auth: &AuthUser, owned: &HashSet<String>, merchant_id: &str) -> bool {
    is_admin(auth) || owned.contains(merchant_id)
}

/// Serialises an event's payload exactly as it would be sent to a
/// merchant's webhook endpoint.
///
/// This is a pure helper (no state, no I/O) so the worker, tests, and
/// debugging views all render the same preview for a given event.
/// A `serde_json::Value` always serialises successfully; the fallback
/// only guards against a future payload type change.
pub fn event_payload_preview(event: &WebhookEvent) -> String {
    serde_json::to_string(&event.payload).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Lists webhook events for merchants owned by the caller.
///
/// Admins see events for every merchant. Results are newest first
/// (by `created_at`, descending) and capped at 100 entries. When
/// `merchant_id` is given the result is restricted to that merchant,
/// still subject to the ownership rule above.
pub async fn list_events(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<Vec<WebhookEvent>>, ApiError> {
    let owned = owned_merchant_ids(&state, &auth).await;

    let mut events: Vec<WebhookEvent> = state
        .webhook_events
        .read()
        .await
        .iter()
        .filter(|event| may_see(&auth, &owned, &event.merchant_id))
        .filter(|event| {
            query
                .merchant_id
                .as_deref()
                .is_none_or(|merchant_id| event.merchant_id == merchant_id)
        })
        .cloned()
        .collect();

    // Newest first, capped so one response stays bounded.
    events.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    if query.page.is_some() || query.per_page.is_some() {
        events = crate::pagination::apply_page(events, query.page, query.per_page);
    } else {
        events.truncate(100);
    }

    Ok(Json(events))
}

/// Lists webhook deliveries visible to the caller.
///
/// A delivery is visible when its event belongs to a merchant owned
/// by the caller (admins see all deliveries). When `event_id` is
/// given the result is restricted to deliveries of that one event.
/// Results are newest first (most recently recorded first).
pub async fn list_deliveries(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ListDeliveriesQuery>,
) -> Result<Json<Vec<WebhookDelivery>>, ApiError> {
    let owned = owned_merchant_ids(&state, &auth).await;

    // Ids of the events this caller is allowed to see; deliveries are
    // visible exactly when their event is in this set.
    let visible_event_ids: HashSet<String> = state
        .webhook_events
        .read()
        .await
        .iter()
        .filter(|event| may_see(&auth, &owned, &event.merchant_id))
        .map(|event| event.id.clone())
        .collect();

    let deliveries: Vec<WebhookDelivery> = state
        .deliveries
        .read()
        .await
        .iter()
        .rev()
        .filter(|delivery| visible_event_ids.contains(&delivery.event_id))
        .filter(|delivery| {
            query
                .event_id
                .as_deref()
                .is_none_or(|event_id| delivery.event_id == event_id)
        })
        .cloned()
        .collect();
    let deliveries = crate::pagination::apply_page(deliveries, query.page, query.per_page);

    Ok(Json(deliveries))
}

/// Queues a fresh delivery attempt for an existing event.
///
/// The event must exist and belong to a merchant owned by the caller
/// (admins may redeliver any merchant's events), and that merchant
/// must have a webhook URL configured. On success a new
/// [`WebhookDelivery`] in [`DeliveryStatus::Pending`] with zero
/// attempts and `next_retry_at` set to now is recorded and returned
/// with `202 Accepted`.
///
/// Note: this handler does **not** send any HTTP request. Actually
/// POSTing the (signed) payload to the merchant's webhook URL is the
/// background worker's job; this handler only enqueues the delivery
/// for the worker to pick up.
pub async fn redeliver(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<(StatusCode, Json<WebhookDelivery>), ApiError> {
    // Find the event to redeliver.
    let event = state
        .webhook_events
        .read()
        .await
        .iter()
        .find(|event| event.id == event_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("webhook event {event_id} not found")))?;

    // Resolve its merchant and enforce ownership + webhook URL.
    let merchant = state
        .merchants
        .read()
        .await
        .values()
        .find(|merchant| merchant.id == event.merchant_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("merchant {} not found", event.merchant_id)))?;

    if !is_admin(&auth) && merchant.owner_user_id != auth.user_id {
        return Err(ApiError::Forbidden(
            "webhook event belongs to a merchant you do not own".to_string(),
        ));
    }

    let webhook_url = merchant
        .webhook_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| ApiError::BadRequest("merchant has no webhook_url".to_string()))?
        .to_string();

    let delivery = WebhookDelivery {
        id: ids::webhook_event_id(),
        event_id: event.id.clone(),
        url: webhook_url,
        status: DeliveryStatus::Pending,
        attempts: 0,
        last_status_code: None,
        next_retry_at: Some(Utc::now()),
    };

    state.deliveries.write().await.push(delivery.clone());

    // 202 Accepted: the delivery is queued, not yet sent — the worker
    // performs the actual HTTP send (see the handler docs above).
    Ok((StatusCode::ACCEPTED, Json(delivery)))
}

/// Checks a webhook signature without delivering anything.
///
/// Requires authentication (the `AuthUser` extractor rejects
/// unauthenticated calls before this handler runs). Returns
/// `{ "valid": true }` when `signature` is the hex-encoded
/// HMAC-SHA256 of `payload` keyed by `secret`, and
/// `{ "valid": false }` otherwise — including for malformed
/// signatures, which the domain verifier rejects without panicking.
pub async fn test_signature(
    _auth: AuthUser,
    State(_state): State<AppState>,
    Json(req): Json<SigReq>,
) -> Result<Json<SigResp>, ApiError> {
    let valid = verify_signature(&req.secret, req.payload.as_bytes(), &req.signature);
    Ok(Json(SigResp { valid }))
}
