//! Provider webhook / callback receivers (Waves 80–82).
//!
//! These are the inbound endpoints providers call — distinct from
//! the merchant webhook *management* API in `handlers::webhooks`:
//!
//! - `POST /api/v1/webhooks/stripe` (W80) — Stripe events, verified
//!   with the `Stripe-Signature` HMAC scheme against
//!   `STRIPE_WEBHOOK_SECRET`. Without the secret the endpoint
//!   answers 503: an unverifiable webhook must never settle money.
//! - `POST /api/v1/webhooks/paypal` (W81) — PayPal events, verified
//!   through PayPal's `verify-webhook-signature` API (needs
//!   `PAYPAL_CLIENT_ID` / `PAYPAL_CLIENT_SECRET` /
//!   `PAYPAL_WEBHOOK_ID`); unconfigured → 503.
//! - `POST /api/v1/callbacks/bkash` (W82) — bKash tokenized
//!   callback. On a success callback with bKash credentials
//!   configured, the payment is *executed* against bKash and only
//!   an execute verdict of `Completed` settles the transaction.
//!   Without credentials the callback is recorded and answered
//!   `verified: false` — accepted, never settled on trust.
//!
//! Settlement mirrors the SMS path: status → Completed,
//! `provider_trx_id` recorded, audit entry + `payment.completed`
//! merchant webhook emitted.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

use miraclepay_domain::transaction::TxStatus;

use crate::dto::ApiError;
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

/// Verify a Stripe `Stripe-Signature` header:
/// `t=<unix>,v1=<hex hmac-sha256 of "{t}.{body}">`, with a
/// 300-second tolerance window (Stripe's own recommendation).
/// Pure function of (secret, header, body, now) — unit-tested.
pub fn verify_stripe_signature(
    secret: &str,
    header: &str,
    body: &str,
    now_unix: i64,
) -> Result<(), ApiError> {
    let mut timestamp: Option<i64> = None;
    let mut signatures: Vec<String> = Vec::new();
    for part in header.split(',') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("t=") {
            timestamp = v.parse().ok();
        } else if let Some(v) = part.strip_prefix("v1=") {
            signatures.push(v.to_string());
        }
    }
    let timestamp = timestamp
        .ok_or_else(|| ApiError::BadRequest("stripe signature: missing timestamp".to_string()))?;
    if (now_unix - timestamp).abs() > 300 {
        return Err(ApiError::BadRequest(
            "stripe signature: timestamp outside tolerance".to_string(),
        ));
    }
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::Internal("stripe signature: bad secret".to_string()))?;
    mac.update(format!("{timestamp}.{body}").as_bytes());
    let expected = mac.finalize().into_bytes();
    let matched = signatures.iter().any(|sig| {
        let Ok(bytes) = hex_decode(sig) else {
            return false;
        };
        // Constant-time comparison via the MAC verifier on a
        // re-keyed instance is overkill; compare with a
        // length-checked XOR fold.
        bytes.len() == expected.len()
            && bytes
                .iter()
                .zip(expected.iter())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    });
    if matched {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    if !s.len().is_multiple_of(2) {
        return Err(());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

/// Settle the open transaction identified by `reference` (matches
/// the transaction `reference`, its id, or an existing provider
/// trx id). Returns the settled transaction id, or `None` when no
/// open transaction matches (already settled counts as matched —
/// providers retry, and a duplicate completion is a no-op success).
pub async fn settle_by_reference(
    state: &AppState,
    reference: &str,
    provider_trx_id: Option<String>,
) -> Option<String> {
    let mut transactions = state.transactions.write().await;
    let tx = transactions.values_mut().find(|tx| {
        tx.reference.as_deref() == Some(reference)
            || tx.id == reference
            || tx.provider_trx_id.as_deref() == Some(reference)
    })?;
    if tx.status == TxStatus::Completed {
        return Some(tx.id.clone());
    }
    if tx.status != TxStatus::Pending && tx.status != TxStatus::Processing {
        return None;
    }
    tx.status = TxStatus::Completed;
    if provider_trx_id.is_some() {
        tx.provider_trx_id = provider_trx_id;
    }
    tx.updated_at = Utc::now();
    let settled = tx.clone();
    drop(transactions);
    state.audit(
        None,
        Some(settled.merchant_id.clone()),
        "payment.completed.provider_webhook",
        "transaction",
        Some(settled.id.clone()),
    );
    // v0.0.4 Stream D (site S3): the merchant's opted-in webhook
    // transformer runs over the payload before the event is
    // stored (bounded, original kept on any failure), and
    // notification plugins are told about the completion
    // (fire-and-forget). The already-Completed early return above
    // is the dedupe: a duplicate provider webhook fires neither.
    let payload = crate::plugins::apply_webhook_transformer(
        state,
        &settled.merchant_id,
        "payment.completed",
        json!({
            "transaction_id": settled.id,
            "amount_minor": settled.amount.amount_minor,
            "currency": settled.amount.currency.to_string(),
            "reference": settled.reference,
            "provider_trx_id": settled.provider_trx_id,
        }),
    )
    .await;
    let event_id = miraclepay_domain::ids::webhook_event_id();
    state
        .webhook_events
        .write()
        .await
        .push(miraclepay_domain::webhook::WebhookEvent {
            id: event_id.clone(),
            merchant_id: settled.merchant_id.clone(),
            event_type: "payment.completed".to_string(),
            payload,
            created_at: Utc::now(),
        });
    crate::plugins::dispatch_notification(
        state,
        crate::plugins::notification_arg("payment.completed", &event_id, &settled, None),
    )
    .await;
    Some(settled.id)
}

/// `POST /api/v1/webhooks/stripe` (Wave 80).
pub async fn stripe_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<impl IntoResponse, ApiError> {
    let secret = std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default();
    if secret.trim().is_empty() {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "stripe webhook not configured (STRIPE_WEBHOOK_SECRET)"})),
        ));
    }
    let signature = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;
    verify_stripe_signature(&secret, signature, &body, Utc::now().timestamp())?;
    let event: Value = serde_json::from_str(&body)
        .map_err(|_| ApiError::BadRequest("stripe webhook: invalid JSON".to_string()))?;
    let mut settled = None;
    if event["type"] == "checkout.session.completed" {
        let object = &event["data"]["object"];
        let reference = object["client_reference_id"]
            .as_str()
            .or_else(|| object["metadata"]["reference"].as_str());
        if let Some(reference) = reference {
            settled = settle_by_reference(
                &state,
                reference,
                object["payment_intent"].as_str().map(str::to_string),
            )
            .await;
        }
    }
    Ok((
        StatusCode::OK,
        Json(json!({"received": true, "settled": settled})),
    ))
}

/// `POST /api/v1/webhooks/paypal` (Wave 81).
pub async fn paypal_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<impl IntoResponse, ApiError> {
    use miraclepay_gateways::paypal_checkout::PayPalConfig;
    use miraclepay_gateways::transport::{HttpTransport, ReqwestTransport};

    let webhook_id = std::env::var("PAYPAL_WEBHOOK_ID").unwrap_or_default();
    let config = PayPalConfig::from_env();
    if webhook_id.trim().is_empty() || config.is_err() {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                json!({"error": "paypal webhook not configured (PAYPAL_CLIENT_ID/PAYPAL_CLIENT_SECRET/PAYPAL_WEBHOOK_ID)"}),
            ),
        ));
    }
    let config = config.expect("checked above");
    let event: Value = serde_json::from_str(&body)
        .map_err(|_| ApiError::BadRequest("paypal webhook: invalid JSON".to_string()))?;
    let header_value = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    let verify_payload = json!({
        "auth_algo": header_value("paypal-auth-algo"),
        "cert_url": header_value("paypal-cert-url"),
        "transmission_id": header_value("paypal-transmission-id"),
        "transmission_sig": header_value("paypal-transmission-sig"),
        "transmission_time": header_value("paypal-transmission-time"),
        "webhook_id": webhook_id,
        "webhook_event": event,
    });
    // Verification is a blocking HTTP round trip (token + verify);
    // keep it off the async runtime's worker threads.
    let verified = tokio::task::spawn_blocking(move || -> Result<bool, ApiError> {
        let transport = ReqwestTransport::new()
            .map_err(|e| ApiError::Internal(format!("paypal transport: {e}")))?;
        let token_body = {
            let spec = config.token_request();
            let (status, body) = transport
                .send(&spec)
                .map_err(|e| ApiError::Internal(format!("paypal token: {e}")))?;
            if !(200..300).contains(&status) {
                return Err(ApiError::Internal(format!("paypal token HTTP {status}")));
            }
            body
        };
        let token = miraclepay_gateways::paypal_checkout::parse_token(&token_body)
            .map_err(|e| ApiError::Internal(format!("paypal token parse: {e}")))?;
        let spec = miraclepay_gateways::bkash_tokenized::HttpRequestSpec {
            method: "POST",
            url: format!(
                "{}/v1/notifications/verify-webhook-signature",
                config.base_url()
            ),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Bearer {}", token.access_token),
                ),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: verify_payload.to_string(),
        };
        let (status, body) = transport
            .send(&spec)
            .map_err(|e| ApiError::Internal(format!("paypal verify: {e}")))?;
        if !(200..300).contains(&status) {
            return Ok(false);
        }
        let verdict: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        Ok(verdict["verification_status"] == "SUCCESS")
    })
    .await
    .map_err(|_| ApiError::Internal("paypal verify task failed".to_string()))??;
    if !verified {
        return Err(ApiError::Unauthorized);
    }
    let mut settled = None;
    if event["event_type"] == "PAYMENT.CAPTURE.COMPLETED" {
        let resource = &event["resource"];
        if let Some(custom_id) = resource["custom_id"].as_str() {
            settled = settle_by_reference(
                &state,
                custom_id,
                resource["id"].as_str().map(str::to_string),
            )
            .await;
        }
    }
    Ok((
        StatusCode::OK,
        Json(json!({"received": true, "settled": settled})),
    ))
}

/// `POST /api/v1/callbacks/bkash` (Wave 82).
pub async fn bkash_callback(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    use miraclepay_gateways::bkash_tokenized::BkashTokenizedConfig;
    use miraclepay_gateways::transport as gateway_transport;

    let payment_id = body["paymentID"].as_str().unwrap_or_default().to_string();
    let status = body["status"].as_str().unwrap_or_default().to_string();
    let invoice = body["merchantInvoiceNumber"]
        .as_str()
        .or_else(|| body["reference"].as_str())
        .unwrap_or_default()
        .to_string();
    if status != "success" {
        return Ok((
            StatusCode::OK,
            Json(
                json!({"accepted": true, "verified": false, "reason": format!("callback status: {status}")}),
            ),
        ));
    }
    let reference = if !invoice.is_empty() {
        invoice.clone()
    } else {
        payment_id.clone()
    };
    // With credentials, confirm with bKash itself (grant + execute)
    // before settling; without them, record and report honestly.
    let config = BkashTokenizedConfig::from_env();
    let Ok(config) = config else {
        state.audit(
            None,
            None,
            "payment.bkash_callback_unverified",
            "transaction",
            Some(reference.clone()),
        );
        return Ok((
            StatusCode::ACCEPTED,
            Json(
                json!({"accepted": true, "verified": false, "reason": "bkash credentials not configured; not settled"}),
            ),
        ));
    };
    let executed = tokio::task::spawn_blocking(move || {
        let transport = gateway_transport::ReqwestTransport::new()?;
        let token = gateway_transport::bkash_grant_token(&transport, &config)?;
        gateway_transport::bkash_execute_payment(&transport, &config, &token.id_token, &payment_id)
    })
    .await
    .map_err(|_| ApiError::Internal("bkash execute task failed".to_string()))?
    .map_err(|e| ApiError::BadRequest(format!("bkash execute: {e}")))?;
    if executed["transactionStatus"] == "Completed" {
        let settled = settle_by_reference(
            &state,
            &reference,
            executed["trxID"].as_str().map(str::to_string),
        )
        .await;
        return Ok((
            StatusCode::OK,
            Json(json!({"accepted": true, "verified": true, "settled": settled})),
        ));
    }
    Ok((
        StatusCode::OK,
        Json(
            json!({"accepted": true, "verified": false, "reason": format!("execute transactionStatus: {}", executed["transactionStatus"])}),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::Mac;

    fn sign(secret: &str, timestamp: i64, body: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{timestamp}.{body}").as_bytes());
        let sig = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        format!("t={timestamp},v1={sig}")
    }

    #[test]
    fn stripe_signature_round_trip_and_tamper() {
        let body = r#"{"type":"checkout.session.completed"}"#;
        let now = 1_700_000_000;
        let header = sign("whsec_test", now, body);
        assert!(verify_stripe_signature("whsec_test", &header, body, now).is_ok());
        // Tampered body fails.
        assert!(verify_stripe_signature("whsec_test", &header, "{}", now).is_err());
        // Wrong secret fails.
        assert!(verify_stripe_signature("whsec_other", &header, body, now).is_err());
        // Stale timestamp fails (replay window).
        let stale = sign("whsec_test", now - 3600, body);
        assert!(verify_stripe_signature("whsec_test", &stale, body, now).is_err());
    }

    #[tokio::test]
    async fn settle_by_reference_completes_open_transaction() {
        let state = AppState::new("s".to_string());
        let tx = miraclepay_domain::transaction::Transaction {
            id: "txn_1".to_string(),
            merchant_id: "mrc_1".to_string(),
            gateway: "stripe".to_string(),
            amount: miraclepay_domain::money::Money::new(
                5000,
                miraclepay_domain::money::Currency::USD,
            ),
            status: TxStatus::Processing,
            customer_email: None,
            customer_phone: None,
            sender_number: None,
            provider_trx_id: None,
            reference: Some("ref-42".to_string()),
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state
            .transactions
            .write()
            .await
            .insert("txn_1".to_string(), tx);
        let settled = settle_by_reference(&state, "ref-42", Some("pi_123".to_string())).await;
        assert_eq!(settled.as_deref(), Some("txn_1"));
        let transactions = state.transactions.read().await;
        assert_eq!(transactions["txn_1"].status, TxStatus::Completed);
        assert_eq!(
            transactions["txn_1"].provider_trx_id.as_deref(),
            Some("pi_123")
        );
        // Idempotent: a provider retry reports the same settlement.
        drop(transactions);
        let again = settle_by_reference(&state, "ref-42", None).await;
        assert_eq!(again.as_deref(), Some("txn_1"));
    }
}
