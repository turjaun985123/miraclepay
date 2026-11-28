//! SMS ingestion handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! This is the server side of the Android SMS-forwarder app: when a
//! payment-confirmation SMS arrives on a merchant's phone, the app
//! POSTs it here verbatim as a
//! [`miraclepay_sms_verify::types::IncomingSms`], and this module
//! decides what the message means.
//!
//! Endpoints (routes are wired up by the caller):
//!
//! * `receive_sms` — verify one forwarded SMS against the
//!   authenticated merchant's pending / processing transactions and,
//!   on an exact match, settle the transaction.
//! * `parse_preview` — dry run: return the structured parse of one
//!   forwarded SMS without matching or mutating anything, so the app
//!   (or a merchant debugging a device) can see what the server
//!   extracts from a message.
//!
//! # Authentication
//!
//! Both endpoints are device endpoints, not user endpoints: there is
//! no JWT. The caller presents its raw API key in the `x-device-key`
//! header. Only the key's hash is ever looked up — the raw key is
//! hashed with [`miraclepay_domain::api_key::hash_key`] and matched
//! against the `api_keys` map in [`AppState`], which is keyed by that
//! hash. The key must exist, must not be revoked, and must carry the
//! [`ApiScope::PaymentsWrite`] scope (or
//! [`ApiScope::FullAccess`], which allows every scope). Anything else
//! is [`ApiError::Unauthorized`]. The authenticated key also decides
//! *whose* transactions an SMS may settle: its `merchant_id` is the
//! only merchant considered, so one merchant's device can never
//! complete another merchant's payment.
//!
//! # Response contract of `receive_sms`
//!
//! Verification outcomes are not errors, so they are reported in the
//! response body with a deliberate status split:
//!
//! | Outcome       | Status | Body                                              |
//! |---------------|--------|---------------------------------------------------|
//! | `verified`    | 200    | `{"outcome":"verified","transaction_id":…}`       |
//! | `probable`    | 200    | `{"outcome":"probable","transaction_id":…}`       |
//! | `duplicate`   | 200    | `{"outcome":"duplicate"}`                         |
//! | `unmatched`   | 202    | `{"outcome":"unmatched"}`                         |
//! | `not_payment` | 202    | `{"outcome":"not_payment"}`                       |
//!
//! 200 means "accepted and fully dealt with"; 202 means "accepted,
//! but this message settles nothing" (it matched no pending payment,
//! or it is not a payment SMS at all). Only `verified` mutates state:
//! the transaction is completed, its provider transaction id is
//! recorded and reserved in `used_trx` so the same confirmation can
//! never settle a second payment, and a `payment.completed` webhook
//! event is queued for the merchant.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::Utc;
use serde_json::{json, Value};

use miraclepay_domain::api_key::{hash_key, ApiKey, ApiScope};
use miraclepay_domain::ids;
use miraclepay_domain::transaction::TxStatus;
use miraclepay_domain::webhook::WebhookEvent;
use miraclepay_sms_verify::types::IncomingSms;
use miraclepay_sms_verify::verifier::{parse_incoming, verify_sms, VerificationOutcome};

use crate::dto::ApiError;
use crate::state::AppState;

/// Header the Android forwarder uses to present its raw API key.
const DEVICE_KEY_HEADER: &str = "x-device-key";

// ---------------------------------------------------------------------------
// Device authentication
// ---------------------------------------------------------------------------

/// Authenticate a device request from its `x-device-key` header.
///
/// Returns the matching [`ApiKey`] (cloned out of the store, so the
/// caller holds no lock while it works) or
/// [`ApiError::Unauthorized`] when:
///
/// * the header is missing or is not valid header text,
/// * no key with the presented key's hash exists,
/// * the key has been revoked, or
/// * none of the key's scopes allows payment writes — a read-only
///   key must not be able to settle payments by forwarding SMS.
///
/// The raw key is hashed immediately and never stored, logged, or
/// echoed back.
async fn authenticate_device(state: &AppState, headers: &HeaderMap) -> Result<ApiKey, ApiError> {
    let raw_key = headers
        .get(DEVICE_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::Unauthorized)?;

    let key_hash = hash_key(raw_key);
    let key = state
        .api_keys
        .read()
        .await
        .get(&key_hash)
        .cloned()
        .ok_or(ApiError::Unauthorized)?;

    // `ApiKey::allows` already rejects revoked keys, and
    // `ApiScope::FullAccess` allows every required scope, so asking
    // for `PaymentsWrite` covers the `FullAccess` case as well; both
    // are spelled out here because they are the two scopes the
    // contract names.
    let scope_ok = key.allows(ApiScope::PaymentsWrite) || key.allows(ApiScope::FullAccess);
    if key.revoked || !scope_ok {
        return Err(ApiError::Unauthorized);
    }

    // Device presence (Wave 36): record successful use so the
    // dashboard can show when a device last checked in. Throttled
    // to one write per minute per key — presence granularity, not
    // an audit trail (the audit log covers individual calls).
    let now = chrono::Utc::now();
    let stale = key
        .last_used_at
        .map(|last| (now - last).num_seconds() >= 60)
        .unwrap_or(true);
    if stale {
        if let Some(stored) = state.api_keys.write().await.get_mut(&key_hash) {
            stored.last_used_at = Some(now);
        }
    }

    Ok(key)
}

// ---------------------------------------------------------------------------
// POST — receive one forwarded SMS and verify it
// ---------------------------------------------------------------------------

/// Receive one SMS forwarded by the Android app and verify it
/// against the authenticated merchant's open transactions.
///
/// See the module documentation for the full response contract. In
/// short: an exact verification completes the transaction, reserves
/// its provider transaction id against reuse, and queues a
/// `payment.completed` webhook event; every other outcome is
/// reported without mutating any transaction.
pub async fn receive_sms(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(sms): Json<IncomingSms>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let api_key = authenticate_device(&state, &headers).await?;
    let merchant_id = api_key.merchant_id.clone();

    // Snapshot the merchant's open transactions: only payments still
    // awaiting confirmation (Pending or Processing) can be settled
    // by an SMS. Transactions of every other merchant are invisible
    // to this device.
    let pending: Vec<_> = state
        .transactions
        .read()
        .await
        .values()
        .filter(|tx| {
            tx.merchant_id == merchant_id
                && matches!(tx.status, TxStatus::Pending | TxStatus::Processing)
        })
        .cloned()
        .collect();

    // Snapshot the already-used provider transaction ids so the
    // verifier can reject a re-forwarded confirmation as a duplicate
    // instead of settling a second payment with it.
    let used_trx_snapshot = state.used_trx.read().await.clone();

    let outcome = verify_sms(&sms, &pending, &used_trx_snapshot);

    match outcome {
        VerificationOutcome::Verified(tx_id) => {
            // The verifier matched on a parse of this same SMS, but
            // the provider transaction id and sender live only on
            // that parse, so re-parse to recover them for the
            // record. If the re-parse somehow fails, nothing may be
            // settled on faith: report the SMS as unmatched instead.
            let parsed = match parse_incoming(&sms) {
                Ok(parsed) => parsed,
                Err(_) => {
                    return Ok((
                        StatusCode::ACCEPTED,
                        Json(json!({ "outcome": "unmatched" })),
                    ));
                }
            };

            // Complete the transaction and capture its post-update
            // shape for the webhook payload.
            let completed = {
                let mut transactions = state.transactions.write().await;
                match transactions.get_mut(&tx_id) {
                    Some(tx) if tx.merchant_id == merchant_id => {
                        tx.status = TxStatus::Completed;
                        if let Some(trx_id) = parsed.trx_id.clone() {
                            tx.provider_trx_id = Some(trx_id);
                        }
                        if let Some(sender_number) = parsed.sender_number.clone() {
                            tx.sender_number = Some(sender_number);
                        }
                        tx.updated_at = Utc::now();
                        Some(tx.clone())
                    }
                    _ => None,
                }
            };

            // The transaction vanished (or changed merchant) between
            // matching and mutation: nothing was settled, so report
            // unmatched rather than claiming a verification.
            let Some(completed) = completed else {
                return Ok((
                    StatusCode::ACCEPTED,
                    Json(json!({ "outcome": "unmatched" })),
                ));
            };

            // Reserve the provider transaction id so this same
            // confirmation SMS can never settle another payment.
            if let Some(trx_id) = parsed.trx_id {
                state.used_trx.write().await.insert(trx_id);
            }

            // Notify the merchant through the webhook pipeline.
            //
            // Stream D hooks (D3, spec §3.3 site S1 + §2.2 site S1),
            // in the spec's fixed order: the transaction is already
            // mutated above → build the payload → transform → push
            // the webhook event → fire the notification.
            //
            // Transform: only the event's *payload* passes through
            // the merchant's opted-in webhook transformer via
            // `apply_webhook_transformer` (D2). The envelope (`id`,
            // `merchant_id`, `event_type`, `created_at`) is built
            // here and never transformed. The call is awaited but
            // hard-bounded (spec §3.4: min(manifest timeout,
            // 2000 ms)); every failure mode returns the original
            // payload, so it can never fail or drop this settlement
            // or its webhook event.
            let payload = json!({
                "transaction_id": completed.id,
                "merchant_id": completed.merchant_id,
                "gateway": completed.gateway,
                "amount_minor": completed.amount.amount_minor,
                "currency": completed.amount.currency.code(),
                "status": completed.status,
                "provider_trx_id": completed.provider_trx_id,
                "sender_number": completed.sender_number,
                "reference": completed.reference,
            });
            let payload = crate::plugins::apply_webhook_transformer(
                &state,
                &merchant_id,
                "payment.completed",
                payload,
            )
            .await;
            // Hoisted so the notification below can correlate with
            // this webhook event via `event_id` (spec §2.1).
            let event_id = ids::webhook_event_id();
            state.webhook_events.write().await.push(WebhookEvent {
                id: event_id.clone(),
                merchant_id: merchant_id.clone(),
                event_type: "payment.completed".to_string(),
                payload,
                created_at: Utc::now(),
            });

            // Notification: fire-and-forget (spec §2.3).
            // `dispatch_notification` (D2) spawns the delivery task
            // and returns as soon as it is spawned — the `.await`
            // here never waits for a plugin, never surfaces an
            // error, and holds no state lock (the guards above are
            // already dropped; the arg is built from the `completed`
            // clone). The argument comes from D2's
            // `notification_arg` constructor and is built from the
            // TRANSACTION, never from the (possibly transformed)
            // webhook payload, so notification plugins always see
            // the canonical field set.
            crate::plugins::dispatch_notification(
                &state,
                crate::plugins::notification_arg("payment.completed", &event_id, &completed, None),
            )
            .await;

            Ok((
                StatusCode::OK,
                Json(json!({
                    "outcome": "verified",
                    "transaction_id": completed.id,
                })),
            ))
        }

        VerificationOutcome::ProbableMatch(tx_id) => {
            // A likely match is reported for a human / follow-up step
            // to settle; the automatic path must not complete a
            // transaction on a guess, and the provider transaction
            // id is not reserved.
            Ok((
                StatusCode::OK,
                Json(json!({
                    "outcome": "probable",
                    "transaction_id": tx_id,
                })),
            ))
        }

        VerificationOutcome::Duplicate => {
            Ok((StatusCode::OK, Json(json!({ "outcome": "duplicate" }))))
        }

        VerificationOutcome::Unmatched => {
            plugin_fallback(&state, &merchant_id, &sms, &pending, &used_trx_snapshot).await
        }

        VerificationOutcome::NotPayment => {
            plugin_fallback(&state, &merchant_id, &sms, &pending, &used_trx_snapshot).await
        }
    }
}

/// Wave 119: when the native parsers cannot match a message, try
/// the enabled SMS-parser plugins. A plugin parse that matches a
/// pending transaction exactly settles it through the same steps
/// as the native path (complete, reserve the trx id, webhook);
/// anything else keeps the native outcome's answer.
async fn plugin_fallback(
    state: &AppState,
    merchant_id: &str,
    sms: &IncomingSms,
    pending: &[miraclepay_domain::transaction::Transaction],
    used_trx_snapshot: &std::collections::HashSet<String>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let unmatched = || {
        Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "outcome": "unmatched" })),
        ))
    };
    let Some(parsed) = crate::plugins::plugin_parse_sms(state, sms).await else {
        return unmatched();
    };
    use miraclepay_sms_verify::matcher::{match_transaction, MatchOutcome};
    let tx_id = match match_transaction(&parsed, pending, used_trx_snapshot) {
        MatchOutcome::Exact(tx_id) => tx_id,
        _ => return unmatched(),
    };
    let completed = {
        let mut transactions = state.transactions.write().await;
        match transactions.get_mut(&tx_id) {
            Some(tx) if tx.merchant_id == merchant_id => {
                tx.status = TxStatus::Completed;
                if let Some(trx_id) = parsed.trx_id.clone() {
                    tx.provider_trx_id = Some(trx_id);
                }
                if let Some(sender_number) = parsed.sender_number.clone() {
                    tx.sender_number = Some(sender_number);
                }
                tx.updated_at = Utc::now();
                Some(tx.clone())
            }
            _ => None,
        }
    };
    let Some(completed) = completed else {
        return unmatched();
    };
    if let Some(trx_id) = parsed.trx_id {
        state.used_trx.write().await.insert(trx_id);
    }
    // Stream D hooks (D3, spec §3.3 site S2 + §2.2 site S2): the
    // same treatment as the native settle path, in the same fixed
    // order — build the payload → transform → push → notify.
    //
    // Transform: only this event's `payload` value (the `json!`
    // object below, including its `"via": "sms_plugin_parser"`
    // marker) passes through the merchant's opted-in webhook
    // transformer via `apply_webhook_transformer` (D2); the
    // envelope fields are never transformed. Awaited but
    // hard-bounded (spec §3.4); every failure mode keeps the
    // original payload.
    let payload = json!({
        "transaction_id": completed.id,
        "merchant_id": completed.merchant_id,
        "gateway": completed.gateway,
        "amount_minor": completed.amount.amount_minor,
        "currency": completed.amount.currency.code(),
        "status": completed.status,
        "provider_trx_id": completed.provider_trx_id,
        "sender_number": completed.sender_number,
        "reference": completed.reference,
        "via": "sms_plugin_parser",
    });
    let payload =
        crate::plugins::apply_webhook_transformer(state, merchant_id, "payment.completed", payload)
            .await;
    // Hoisted so the notification below can correlate with this
    // webhook event via `event_id` (spec §2.1).
    let event_id = ids::webhook_event_id();
    state.webhook_events.write().await.push(WebhookEvent {
        id: event_id.clone(),
        merchant_id: merchant_id.to_string(),
        event_type: "payment.completed".to_string(),
        payload,
        created_at: Utc::now(),
    });
    // Notification: fire-and-forget, exactly as on the native
    // settle path (spec §2.3) — `dispatch_notification` (D2)
    // returns once the delivery task is spawned; the `.await`
    // never waits for a plugin and holds no state lock. The
    // argument is built by D2's `notification_arg` constructor
    // from the TRANSACTION (`completed`), never from the
    // (possibly transformed) webhook payload.
    crate::plugins::dispatch_notification(
        state,
        crate::plugins::notification_arg("payment.completed", &event_id, &completed, None),
    )
    .await;
    Ok((
        StatusCode::OK,
        Json(json!({
            "outcome": "verified",
            "transaction_id": completed.id,
            "via": "plugin_parser",
        })),
    ))
}

// ---------------------------------------------------------------------------
// POST — preview the parse of one forwarded SMS
// ---------------------------------------------------------------------------

/// Parse one forwarded SMS and return the structured result,
/// without matching it against any transaction and without mutating
/// any state.
///
/// On success the body is the serialised
/// [`miraclepay_sms_verify::types::ParsedSms`]. A message that does
/// not parse (blank body, or not a payment SMS) is not an API error —
/// the preview exists precisely to surface that — so it is reported
/// as `{"error": …}` with the parser's message. Authentication
/// failures are still [`ApiError::Unauthorized`], exactly as on
/// [`receive_sms`].
pub async fn parse_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(sms): Json<IncomingSms>,
) -> Result<Json<Value>, ApiError> {
    authenticate_device(&state, &headers).await?;

    match parse_incoming(&sms) {
        Ok(parsed) => Ok(Json(serde_json::to_value(&parsed).unwrap_or(Value::Null))),
        Err(error) => Ok(Json(json!({ "error": error.to_string() }))),
    }
}
