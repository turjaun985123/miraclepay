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
            state.webhook_events.write().await.push(WebhookEvent {
                id: ids::webhook_event_id(),
                merchant_id: merchant_id.clone(),
                event_type: "payment.completed".to_string(),
                payload: json!({
                    "transaction_id": completed.id,
                    "merchant_id": completed.merchant_id,
                    "gateway": completed.gateway,
                    "amount_minor": completed.amount.amount_minor,
                    "currency": completed.amount.currency.code(),
                    "status": completed.status,
                    "provider_trx_id": completed.provider_trx_id,
                    "sender_number": completed.sender_number,
                    "reference": completed.reference,
                }),
                created_at: Utc::now(),
            });

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

        VerificationOutcome::Duplicate => Ok((
            StatusCode::OK,
            Json(json!({ "outcome": "duplicate" })),
        )),

        VerificationOutcome::Unmatched => Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "outcome": "unmatched" })),
        )),

        VerificationOutcome::NotPayment => Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "outcome": "not_payment" })),
        )),
    }
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
        Ok(parsed) => Ok(Json(
            serde_json::to_value(&parsed).unwrap_or(Value::Null),
        )),
        Err(error) => Ok(Json(json!({ "error": error.to_string() }))),
    }
}
