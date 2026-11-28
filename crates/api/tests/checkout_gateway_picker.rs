//! Integration tests for the v0.0.4 Stream B checkout gateway
//! picker contract:
//!
//! * `GET /api/v1/checkout/:id` gains `gateways` — the merchant's
//!   offered gateway ids — and `receiving_numbers` —
//!   `[{gateway, number}]` for the offered gateways that have a
//!   configured `merchant_number` (the same payer-safe shape the
//!   public payment-link view already carries).
//! * `POST /api/v1/checkout/:id/submit` gains an optional
//!   `gateway`: when present it must be one of the merchant's
//!   offered gateways, it is stored on the transaction while the
//!   transaction is still `Pending`, and it drives settlement —
//!   the SMS that settles the payment is matched against the
//!   CHOSEN gateway, not the gateway the transaction was created
//!   with. When absent, behaviour is exactly as before (the
//!   creation gateway is kept).
//! * An unoffered gateway is a `400`, leaving the transaction
//!   `Pending` with its original gateway; and the existing rule
//!   that only a `Pending` transaction may be submitted (`409`
//!   otherwise) means the chosen gateway cannot be changed after
//!   submission.
//!
//! The harness mirrors `integration.rs` / `receiving_number.rs`:
//! the real router from [`miraclepay_api::build_router`] is driven
//! in-process via `tower::ServiceExt::oneshot` over a fresh
//! in-memory state. Payments are created through the public
//! payment-link pay flow (which creates a `Pending` transaction —
//! the only state `submit_checkout` accepts), and settlement goes
//! through the real device-key SMS intake (`POST /api/v1/sms`).

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use miraclepay_api::{build_router, AppState};

/// Signing secret for the throwaway in-memory state of each test.
const TEST_SECRET: &str = "checkout-gateway-picker-test-secret";

/// The merchant's bKash receiving number.
const BKASH_NUMBER: &str = "01711111111";

/// The merchant's Nagad receiving number — deliberately different
/// from [`BKASH_NUMBER`] so picking the wrong gateway's number (or
/// gateway) is observable.
const NAGAD_NUMBER: &str = "01822222222";

/// Customer phone number the payments / SMS are about.
const CUSTOMER_PHONE: &str = "01712345678";

/// Payment amount: 1,250.00 BDT, in minor units (poisha). Both SMS
/// bodies below are for exactly this amount.
const AMOUNT_MINOR: i64 = 125_000;

/// Provider transaction id used on the bKash leg.
const BKASH_TRX_ID: &str = "9HX7K2LMNP";

/// Provider transaction id used on the Nagad leg.
const NAGAD_TRX_ID: &str = "NAG7X2K9P1";

/// A realistic bKash "received money" confirmation SMS body for
/// exactly [`AMOUNT_MINOR`], from [`CUSTOMER_PHONE`] (the same
/// shape `integration.rs` uses). Every value is fictional.
const BKASH_SMS_BODY: &str = "You have received Tk 1,250.00 from 01712345678. \
    TrxID: 9HX7K2LMNP. Balance: Tk 5,430.50. Thank you for using bKash.";

/// A realistic Nagad "received money" confirmation SMS body for
/// exactly [`AMOUNT_MINOR`], from [`CUSTOMER_PHONE`] (written
/// hyphenated, as Nagad formats sender numbers). This is the exact
/// template the native parser corpus pins in
/// `crates/sms_verify/tests/corpus.rs` ("nagad received") and in
/// `crates/sms_verify/src/bd_templates.rs`: sender id `Nagad`,
/// `Trx ID:` label, trailing `- Nagad` signature. Every value is
/// fictional.
const NAGAD_SMS_BODY: &str = "You have received Tk 1,250.00 from 01712-345678. Trx ID: NAG7X2K9P1. Your new balance is Tk 5,430.50. - Nagad";

// ---------------------------------------------------------------------------
// Harness helpers (mirroring integration.rs)
// ---------------------------------------------------------------------------

/// A fresh router over a fresh, empty in-memory state.
fn app() -> Router {
    build_router(AppState::new(TEST_SECRET.to_string()))
}

/// Build one request. `token` becomes `Authorization: Bearer …`,
/// `device_key` becomes the `x-device-key` header, and a JSON `body`
/// is serialised with a `application/json` content type.
fn request(
    method: Method,
    uri: &str,
    token: Option<&str>,
    device_key: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(device_key) = device_key {
        builder = builder.header("x-device-key", device_key);
    }
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    builder.body(body).expect("request must build")
}

/// Dispatch one request through a clone of `app` (clones share the
/// same underlying state) and return `(status, json_body)`.
async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(req)
        .await
        .expect("router must answer in-process requests");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body must be readable");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body must be JSON")
    };
    (status, body)
}

/// Register a user and log in with the same credentials, returning
/// the bearer token. Asserts both steps succeed.
async fn register_and_login(app: &Router, email: &str) -> String {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/auth/register",
            None,
            None,
            Some(json!({
                "name": "Gateway Picker Merchant",
                "email": email,
                "password": "password123",
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register: {body}");

    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({ "email": email, "password": "password123" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login: {body}");
    body["token"]
        .as_str()
        .expect("login response must carry a token")
        .to_string()
}

/// Create a merchant owned by the token's user; returns its id.
async fn create_merchant(app: &Router, token: &str) -> String {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/merchants",
            Some(token),
            None,
            Some(json!({
                "owner_user_id": "ignored-by-handler",
                "business_name": "Gateway Picker Shop",
                "email": "shop@example.com",
                "phone": BKASH_NUMBER,
                "country": "BD",
                "webhook_url": null,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create merchant: {body}");
    body["id"]
        .as_str()
        .expect("merchant response must carry an id")
        .to_string()
}

/// Store one gateway configuration for the merchant (enabled,
/// sandbox) with the given receiving (`merchant_number`) number.
async fn configure_gateway(
    app: &Router,
    token: &str,
    merchant_id: &str,
    gateway: &str,
    merchant_number: &str,
) {
    let (status, body) = send(
        app,
        request(
            Method::PUT,
            &format!("/api/v1/merchants/{merchant_id}/gateways"),
            Some(token),
            None,
            Some(json!({
                "gateway": gateway,
                "merchant_number": merchant_number,
                "api_key": null,
                "api_secret": null,
                "extra": {},
                "sandbox": true,
                "enabled": true,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "configure {gateway}: {body}");
    assert_eq!(body["gateway"], json!(gateway));
}

/// Full setup shared by every test: register, login, merchant, and
/// BOTH gateways configured with distinct receiving numbers.
/// Returns `(token, merchant_id)`.
async fn setup_multi_gateway_merchant(app: &Router, email: &str) -> (String, String) {
    let token = register_and_login(app, email).await;
    let merchant_id = create_merchant(app, &token).await;
    configure_gateway(app, &token, &merchant_id, "bkash", BKASH_NUMBER).await;
    configure_gateway(app, &token, &merchant_id, "nagad", NAGAD_NUMBER).await;
    (token, merchant_id)
}

/// Create a fixed-amount payment link for the merchant; returns
/// its public slug.
async fn create_link(app: &Router, token: &str, merchant_id: &str) -> String {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/payment-links",
            Some(token),
            None,
            Some(json!({
                "merchant_id": merchant_id,
                "title": "Gateway Picker Store Pay",
                "description": "One gateway-picker widget",
                "amount_minor": AMOUNT_MINOR,
                "currency": "BDT",
                "collect_phone": true,
                "collect_email": false,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create link: {body}");
    body["slug"]
        .as_str()
        .expect("link response must carry a slug")
        .to_string()
}

/// Pay a link publicly through `gateway`, creating a `Pending`
/// transaction; returns the transaction id. Asserts the pay
/// response reports the transaction as `pending`.
async fn public_pay(app: &Router, slug: &str, gateway: &str) -> String {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            &format!("/api/v1/payment-links/public/{slug}/pay"),
            None,
            None,
            Some(json!({
                "gateway": gateway,
                "customer_phone": CUSTOMER_PHONE,
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "public pay ({gateway}): {body}"
    );
    assert_eq!(body["status"], json!("pending"), "public pay: {body}");
    body["transaction_id"]
        .as_str()
        .expect("pay response must carry the transaction id")
        .to_string()
}

/// Fetch the public checkout view of a transaction (no auth, as a
/// payer's browser would).
async fn get_checkout(app: &Router, transaction_id: &str) -> Value {
    let (status, body) = send(
        app,
        request(
            Method::GET,
            &format!("/api/v1/checkout/{transaction_id}"),
            None,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "checkout details: {body}");
    body
}

/// Read a transaction back privately via the payments API;
/// returns the `transaction` object.
async fn get_transaction(app: &Router, token: &str, transaction_id: &str) -> Value {
    let (status, body) = send(
        app,
        request(
            Method::GET,
            &format!("/api/v1/payments/{transaction_id}"),
            Some(token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get payment: {body}");
    body["transaction"].clone()
}

/// Submit a checkout with an explicit JSON body and return the raw
/// `(status, body)` — callers assert, so validation cases can
/// inspect failures.
async fn submit_checkout(app: &Router, transaction_id: &str, body: Value) -> (StatusCode, Value) {
    send(
        app,
        request(
            Method::POST,
            &format!("/api/v1/checkout/{transaction_id}/submit"),
            None,
            None,
            Some(body),
        ),
    )
    .await
}

/// Create an API key with the `payments_write` scope and return
/// the raw key — the device key the SMS intake requires.
async fn create_device_key(app: &Router, token: &str, merchant_id: &str) -> String {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/api-keys",
            Some(token),
            None,
            Some(json!({
                "merchant_id": merchant_id,
                "name": "Android SMS forwarder",
                "scopes": ["payments_write"],
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create api key: {body}");
    body["key"]
        .as_str()
        .expect("api key creation must return the raw key once")
        .to_string()
}

/// Forward one confirmation SMS through the device-key intake;
/// returns the raw `(status, body)`.
async fn post_sms(
    app: &Router,
    device_key: &str,
    sender: &str,
    sms_body: &str,
) -> (StatusCode, Value) {
    send(
        app,
        request(
            Method::POST,
            "/api/v1/sms",
            None,
            Some(device_key),
            Some(json!({
                "device_id": "gateway-picker-android-device",
                "sender": sender,
                "body": sms_body,
                "received_at": "2026-11-26T03:16:00Z",
                "sim_slot": null,
            })),
        ),
    )
    .await
}

// ---------------------------------------------------------------------------
// (1) Multi-gateway merchant: checkout details list both offered
//     gateways and both receiving numbers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn checkout_details_list_all_offered_gateways_and_receiving_numbers() {
    let app = app();
    let (token, merchant_id) = setup_multi_gateway_merchant(&app, "picker-list@example.com").await;
    let slug = create_link(&app, &token, &merchant_id).await;

    // The transaction is created through bKash, but the checkout
    // view must offer the merchant's whole gateway set — that is
    // what lets the payer pick a different gateway at submit time.
    let transaction_id = public_pay(&app, &slug, "bkash").await;
    let body = get_checkout(&app, &transaction_id).await;

    assert_eq!(body["gateway"], json!("bkash"), "checkout: {body}");
    assert_eq!(body["status"], json!("pending"), "checkout: {body}");

    let gateways = body["gateways"]
        .as_array()
        .expect("checkout details must carry a gateways list");
    assert_eq!(
        gateways.len(),
        2,
        "exactly the two configured gateways are offered: {body}"
    );
    assert!(
        gateways.iter().any(|g| g == &json!("bkash")),
        "bkash must be offered: {body}"
    );
    assert!(
        gateways.iter().any(|g| g == &json!("nagad")),
        "nagad must be offered: {body}"
    );

    let numbers = body["receiving_numbers"]
        .as_array()
        .expect("checkout details must carry a receiving_numbers list");
    assert_eq!(
        numbers.len(),
        2,
        "both configured receiving numbers are listed: {body}"
    );
    assert!(
        numbers
            .iter()
            .any(|n| n == &json!({"gateway": "bkash", "number": BKASH_NUMBER})),
        "bkash receiving number must be listed: {body}"
    );
    assert!(
        numbers
            .iter()
            .any(|n| n == &json!({"gateway": "nagad", "number": NAGAD_NUMBER})),
        "nagad receiving number must be listed: {body}"
    );

    // The pre-existing single-gateway field still names the
    // creation gateway's number.
    assert_eq!(body["receiving_number"], json!(BKASH_NUMBER), "{body}");
}

// ---------------------------------------------------------------------------
// (2) Settle on the CHOSEN gateway: created as bkash, submitted as
//     nagad, settled by a Nagad SMS — and the settled transaction
//     reads back gateway == "nagad"
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submit_with_chosen_gateway_settles_on_that_gateway() {
    let app = app();
    let (token, merchant_id) =
        setup_multi_gateway_merchant(&app, "picker-chosen@example.com").await;
    let slug = create_link(&app, &token, &merchant_id).await;

    // Creation gateway is bKash.
    let transaction_id = public_pay(&app, &slug, "bkash").await;
    let body = get_checkout(&app, &transaction_id).await;
    assert_eq!(body["gateway"], json!("bkash"), "before submit: {body}");

    // The payer actually paid via Nagad: submit names the Nagad
    // gateway, a Nagad sender number and the Nagad TrxID.
    let (status, body) = submit_checkout(
        &app,
        &transaction_id,
        json!({
            "sender_number": CUSTOMER_PHONE,
            "provider_trx_id": NAGAD_TRX_ID,
            "gateway": "nagad",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "checkout submit: {body}");
    assert_eq!(
        body["transaction"]["status"],
        json!("processing"),
        "submit: {body}"
    );
    assert_eq!(
        body["transaction"]["gateway"],
        json!("nagad"),
        "the chosen gateway must be stored on the transaction: {body}"
    );

    // The checkout view now follows the chosen gateway, including
    // which receiving number it presents.
    let body = get_checkout(&app, &transaction_id).await;
    assert_eq!(body["gateway"], json!("nagad"), "after submit: {body}");
    assert_eq!(body["status"], json!("processing"), "after submit: {body}");
    assert_eq!(
        body["receiving_number"],
        json!(NAGAD_NUMBER),
        "checkout must present the chosen gateway's number: {body}"
    );

    // The matching Nagad-format SMS, forwarded with the device
    // key, verifies the payment.
    let device_key = create_device_key(&app, &token, &merchant_id).await;
    let (status, body) = post_sms(&app, &device_key, "Nagad", NAGAD_SMS_BODY).await;
    assert_eq!(status, StatusCode::OK, "sms intake: {body}");
    assert_eq!(body["outcome"], json!("verified"), "sms outcome: {body}");
    assert_eq!(body["transaction_id"], json!(transaction_id));

    // Settled — and it settled as a NAGAD payment, publicly and
    // privately.
    let body = get_checkout(&app, &transaction_id).await;
    assert_eq!(body["status"], json!("completed"), "checkout: {body}");
    assert_eq!(body["gateway"], json!("nagad"), "checkout: {body}");

    let transaction = get_transaction(&app, &token, &transaction_id).await;
    assert_eq!(transaction["status"], json!("completed"), "{transaction}");
    assert_eq!(
        transaction["gateway"],
        json!("nagad"),
        "settled transaction must read back gateway nagad: {transaction}"
    );
    assert_eq!(
        transaction["provider_trx_id"],
        json!(NAGAD_TRX_ID),
        "{transaction}"
    );
}

// ---------------------------------------------------------------------------
// (3) Backwards compatibility: submit WITHOUT gateway keeps the
//     creation gateway and settles via its SMS as before
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submit_without_gateway_keeps_creation_gateway_and_settles() {
    let app = app();
    let (token, merchant_id) =
        setup_multi_gateway_merchant(&app, "picker-compat@example.com").await;
    let slug = create_link(&app, &token, &merchant_id).await;
    let transaction_id = public_pay(&app, &slug, "bkash").await;

    // No `gateway` key at all — the pre-picker request shape.
    let (status, body) = submit_checkout(
        &app,
        &transaction_id,
        json!({
            "sender_number": CUSTOMER_PHONE,
            "provider_trx_id": BKASH_TRX_ID,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "checkout submit: {body}");
    assert_eq!(
        body["transaction"]["status"],
        json!("processing"),
        "submit: {body}"
    );
    assert_eq!(
        body["transaction"]["gateway"],
        json!("bkash"),
        "omitting gateway must keep the creation gateway: {body}"
    );

    // The bKash SMS settles it exactly as before the picker existed.
    let device_key = create_device_key(&app, &token, &merchant_id).await;
    let (status, body) = post_sms(&app, &device_key, "bKash", BKASH_SMS_BODY).await;
    assert_eq!(status, StatusCode::OK, "sms intake: {body}");
    assert_eq!(body["outcome"], json!("verified"), "sms outcome: {body}");
    assert_eq!(body["transaction_id"], json!(transaction_id));

    let body = get_checkout(&app, &transaction_id).await;
    assert_eq!(body["status"], json!("completed"), "checkout: {body}");
    assert_eq!(body["gateway"], json!("bkash"), "checkout: {body}");

    let transaction = get_transaction(&app, &token, &transaction_id).await;
    assert_eq!(transaction["status"], json!("completed"), "{transaction}");
    assert_eq!(transaction["gateway"], json!("bkash"), "{transaction}");
}

// ---------------------------------------------------------------------------
// (4) Validation: a gateway the merchant does not offer is a 400
//     and leaves the transaction Pending with its original gateway
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submit_with_unoffered_gateway_is_400_and_keeps_original() {
    let app = app();
    let (token, merchant_id) =
        setup_multi_gateway_merchant(&app, "picker-invalid@example.com").await;
    let slug = create_link(&app, &token, &merchant_id).await;
    let transaction_id = public_pay(&app, &slug, "bkash").await;

    // "stripe" is a real gateway id, but this merchant offers only
    // bkash + nagad; "not-a-gateway" is nonsense. Both must be
    // rejected the same way, and neither may move the transaction.
    for unoffered in ["stripe", "not-a-gateway"] {
        let (status, body) = submit_checkout(
            &app,
            &transaction_id,
            json!({
                "sender_number": CUSTOMER_PHONE,
                "provider_trx_id": BKASH_TRX_ID,
                "gateway": unoffered,
            }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "submit with unoffered gateway {unoffered:?} must be 400: {body}"
        );

        let body = get_checkout(&app, &transaction_id).await;
        assert_eq!(
            body["status"],
            json!("pending"),
            "rejected submit must leave the transaction pending: {body}"
        );
        assert_eq!(
            body["gateway"],
            json!("bkash"),
            "rejected submit must keep the original gateway: {body}"
        );
    }

    let transaction = get_transaction(&app, &token, &transaction_id).await;
    assert_eq!(transaction["status"], json!("pending"), "{transaction}");
    assert_eq!(transaction["gateway"], json!("bkash"), "{transaction}");
    assert!(
        transaction["provider_trx_id"].is_null(),
        "rejected submits must not record a provider trx id: {transaction}"
    );
    assert!(
        transaction["sender_number"].is_null(),
        "rejected submits must not record a sender number: {transaction}"
    );

    // The transaction is undamaged: a valid submit still works.
    let (status, body) = submit_checkout(
        &app,
        &transaction_id,
        json!({
            "sender_number": CUSTOMER_PHONE,
            "provider_trx_id": BKASH_TRX_ID,
            "gateway": "bkash",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "valid submit after 400s: {body}");
    assert_eq!(
        body["transaction"]["status"],
        json!("processing"),
        "submit: {body}"
    );
    assert_eq!(body["transaction"]["gateway"], json!("bkash"), "{body}");
}

// ---------------------------------------------------------------------------
// (5) The gateway cannot change after submission: a second submit
//     on the Processing transaction is rejected (409) and the
//     chosen gateway stays
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gateway_cannot_change_after_submission() {
    let app = app();
    let (token, merchant_id) =
        setup_multi_gateway_merchant(&app, "picker-locked@example.com").await;
    let slug = create_link(&app, &token, &merchant_id).await;
    let transaction_id = public_pay(&app, &slug, "bkash").await;

    // First submit picks Nagad and moves the transaction to
    // Processing.
    let (status, body) = submit_checkout(
        &app,
        &transaction_id,
        json!({
            "sender_number": CUSTOMER_PHONE,
            "provider_trx_id": NAGAD_TRX_ID,
            "gateway": "nagad",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "first submit: {body}");
    assert_eq!(body["transaction"]["gateway"], json!("nagad"), "{body}");
    assert_eq!(body["transaction"]["status"], json!("processing"), "{body}");

    // A second submit trying to switch back to bKash hits the
    // existing only-Pending-may-submit rule.
    let (status, body) = submit_checkout(
        &app,
        &transaction_id,
        json!({
            "sender_number": CUSTOMER_PHONE,
            "provider_trx_id": BKASH_TRX_ID,
            "gateway": "bkash",
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "second submit on a processing transaction must be 409: {body}"
    );

    // Nothing about the first submission was overwritten.
    let body = get_checkout(&app, &transaction_id).await;
    assert_eq!(body["status"], json!("processing"), "checkout: {body}");
    assert_eq!(
        body["gateway"],
        json!("nagad"),
        "gateway must stay the chosen one: {body}"
    );

    let transaction = get_transaction(&app, &token, &transaction_id).await;
    assert_eq!(transaction["status"], json!("processing"), "{transaction}");
    assert_eq!(transaction["gateway"], json!("nagad"), "{transaction}");
    assert_eq!(
        transaction["provider_trx_id"],
        json!(NAGAD_TRX_ID),
        "the original submission's trx id must survive: {transaction}"
    );
    assert_eq!(
        transaction["sender_number"],
        json!(CUSTOMER_PHONE),
        "{transaction}"
    );
}
