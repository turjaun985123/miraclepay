//! Integration tests for the v0.0.4 Stream A receiving-number
//! contract:
//!
//! * `GET /api/v1/checkout/:id` gains `receiving_number` — the
//!   `merchant_number` of the transaction's gateway config, or
//!   `null` when the merchant has no config / no number for that
//!   gateway.
//! * `GET /api/v1/payment-links/public/:slug` gains
//!   `receiving_numbers`: `[{gateway, number}]` for the offered
//!   gateways that have a configured `merchant_number`.
//!
//! The source of both fields is `GatewayConfig.merchant_number` in
//! `state.gateway_configs`. A receiving number is payer-safe by
//! design — it is the number the merchant publicly asks payers to
//! send money to — but gateway credentials (`api_key` /
//! `api_secret`) must never appear on these public surfaces. These
//! tests pin both halves of that contract, plus the guarantee that
//! exposing the number does not disturb the existing
//! pay → submit → SMS → completed settle loop.
//!
//! The harness mirrors `integration.rs`: the real router from
//! [`miraclepay_api::build_router`] is driven in-process via
//! `tower::ServiceExt::oneshot` over a fresh in-memory state.

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use miraclepay_api::{build_router, AppState};

/// Signing secret for the throwaway in-memory state of each test.
const TEST_SECRET: &str = "receiving-number-test-secret";

/// The merchant's bKash receiving number.
const BKASH_NUMBER: &str = "01711111111";

/// The merchant's Nagad receiving number — deliberately different
/// from [`BKASH_NUMBER`] so a leak of the wrong gateway's number is
/// observable.
const NAGAD_NUMBER: &str = "01822222222";

/// Distinctive credential values. They are configured on the
/// gateways below and then asserted ABSENT from every public
/// response body: if any of these strings ever appears in a
/// checkout or public-link payload, credentials are leaking.
const BKASH_API_KEY: &str = "bkash-key-DISTINCTIVE-7f3a";
const BKASH_API_SECRET: &str = "bkash-secret-DISTINCTIVE-7f3a";
const NAGAD_API_KEY: &str = "nagad-key-DISTINCTIVE-9c1b";
const NAGAD_API_SECRET: &str = "nagad-secret-DISTINCTIVE-9c1b";
const STRIPE_API_KEY: &str = "stripe-key-DISTINCTIVE-4d8e";
const STRIPE_API_SECRET: &str = "stripe-secret-DISTINCTIVE-4d8e";

/// Every distinctive credential configured anywhere in this file.
const ALL_SECRETS: [&str; 6] = [
    BKASH_API_KEY,
    BKASH_API_SECRET,
    NAGAD_API_KEY,
    NAGAD_API_SECRET,
    STRIPE_API_KEY,
    STRIPE_API_SECRET,
];

/// Customer phone number the payment / SMS are about.
const CUSTOMER_PHONE: &str = "01712345678";

/// Payment amount: 1,250.00 BDT, in minor units (poisha).
const AMOUNT_MINOR: i64 = 125_000;

/// A realistic bKash "received money" confirmation SMS body for
/// exactly [`AMOUNT_MINOR`], from [`CUSTOMER_PHONE`]. Every value in
/// it is fictional and invented for this test.
const BKASH_SMS_BODY: &str = "You have received Tk 1,250.00 from 01712345678. \
    TrxID: 9HX7K2LMNP. Balance: Tk 5,430.50. Thank you for using bKash.";

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
                "name": "Receiving Number Merchant",
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
                "business_name": "Receiving Number Shop",
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
/// sandbox). `merchant_number` / `api_key` / `api_secret` may each
/// be `None` to model a partially-configured gateway.
async fn configure_gateway(
    app: &Router,
    token: &str,
    merchant_id: &str,
    gateway: &str,
    merchant_number: Option<&str>,
    api_key: Option<&str>,
    api_secret: Option<&str>,
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
                "api_key": api_key,
                "api_secret": api_secret,
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

/// Store one gateway configuration with an explicit `enabled`
/// flag (coordinator fix-round helper for the A6 spec tests).
async fn configure_gateway_enabled(
    app: &Router,
    token: &str,
    merchant_id: &str,
    gateway: &str,
    merchant_number: Option<&str>,
    enabled: bool,
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
                "enabled": enabled,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "configure {gateway}: {body}");
}

/// Create a payment of [`AMOUNT_MINOR`] through `gateway` for the
/// merchant; returns the transaction id.
async fn create_payment(app: &Router, token: &str, merchant_id: &str, gateway: &str) -> String {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/payments",
            Some(token),
            None,
            Some(json!({
                "merchant_id": merchant_id,
                "gateway": gateway,
                "amount_minor": AMOUNT_MINOR,
                "currency": "BDT",
                "customer_email": "customer@example.com",
                "customer_phone": CUSTOMER_PHONE,
                "reference": "order-2001",
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create {gateway} payment: {body}"
    );
    body["transaction"]["id"]
        .as_str()
        .expect("payment response must carry a transaction id")
        .to_string()
}

/// Create a fixed-amount payment link for the merchant; returns its
/// public slug.
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
                "title": "Receiving Number Store Pay",
                "description": "One receiving-number widget",
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

/// Fetch the public view of a payment link (no auth).
async fn get_public_link(app: &Router, slug: &str) -> Value {
    let (status, body) = send(
        app,
        request(
            Method::GET,
            &format!("/api/v1/payment-links/public/{slug}"),
            None,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "public link fetch: {body}");
    body
}

/// Assert that none of the distinctive configured credential values
/// appears anywhere in the serialised response body.
fn assert_no_secrets(body: &Value, context: &str) {
    let serialized = body.to_string();
    for secret in ALL_SECRETS {
        assert!(
            !serialized.contains(secret),
            "{context} must not leak credential {secret:?}: {body}"
        );
    }
}

// ---------------------------------------------------------------------------
// (1) Checkout details expose the receiving number of the
//     transaction's gateway
// ---------------------------------------------------------------------------

#[tokio::test]
async fn checkout_exposes_receiving_number_for_configured_gateway() {
    let app = app();
    let token = register_and_login(&app, "recv-checkout@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        Some(BKASH_API_KEY),
        Some(BKASH_API_SECRET),
    )
    .await;

    let transaction_id = create_payment(&app, &token, &merchant_id, "bkash").await;
    let body = get_checkout(&app, &transaction_id).await;

    assert_eq!(body["gateway"], json!("bkash"));
    assert_eq!(
        body["receiving_number"],
        json!(BKASH_NUMBER),
        "checkout must expose the configured merchant_number as \
         receiving_number: {body}"
    );
}

// ---------------------------------------------------------------------------
// (2) The SCALAR number is the selected gateway's — and no
//     credentials anywhere. (Adjudicated at v0.0.4 integration:
//     Stream B's gateway picker deliberately adds a
//     `receiving_numbers` list carrying EVERY offered gateway's
//     number, because the payer must see the number for the
//     gateway they are about to switch to BEFORE submitting.
//     Receiving numbers are the merchant's published pay-to
//     addresses, so listing the offered set is payer-safe; the
//     invariant that survives from Stream A is that the scalar
//     `receiving_number` always names the transaction's own
//     gateway, and that no credential value ever appears.)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn checkout_shows_only_selected_gateway_number_and_no_secrets() {
    let app = app();
    let token = register_and_login(&app, "recv-selected@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        Some(BKASH_API_KEY),
        Some(BKASH_API_SECRET),
    )
    .await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "nagad",
        Some(NAGAD_NUMBER),
        Some(NAGAD_API_KEY),
        Some(NAGAD_API_SECRET),
    )
    .await;

    // A bKash transaction: the scalar receiving_number is the
    // bKash number; the picker list carries both offered numbers;
    // no credential value appears anywhere.
    let bkash_tx = create_payment(&app, &token, &merchant_id, "bkash").await;
    let body = get_checkout(&app, &bkash_tx).await;
    assert_eq!(body["receiving_number"], json!(BKASH_NUMBER), "{body}");
    let listed: Vec<&Value> = body["receiving_numbers"]
        .as_array()
        .map(|entries| entries.iter().collect())
        .unwrap_or_default();
    assert_eq!(listed.len(), 2, "both offered numbers listed: {body}");
    assert!(
        listed
            .iter()
            .any(|e| e["gateway"] == json!("bkash") && e["number"] == json!(BKASH_NUMBER)),
        "bkash entry listed: {body}"
    );
    assert!(
        listed
            .iter()
            .any(|e| e["gateway"] == json!("nagad") && e["number"] == json!(NAGAD_NUMBER)),
        "nagad entry listed for the picker: {body}"
    );
    assert_no_secrets(&body, "bkash checkout");

    // Symmetrically, a Nagad transaction's scalar is the Nagad
    // number, with the same picker list and no credentials.
    let nagad_tx = create_payment(&app, &token, &merchant_id, "nagad").await;
    let body = get_checkout(&app, &nagad_tx).await;
    assert_eq!(body["gateway"], json!("nagad"));
    assert_eq!(body["receiving_number"], json!(NAGAD_NUMBER), "{body}");
    assert_no_secrets(&body, "nagad checkout");
}

// ---------------------------------------------------------------------------
// (3) Public link view lists receiving numbers for configured
//     gateways only; a gateway without a merchant_number is absent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn public_link_lists_receiving_numbers_for_configured_gateways_only() {
    let app = app();
    let token = register_and_login(&app, "recv-link@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        Some(BKASH_API_KEY),
        Some(BKASH_API_SECRET),
    )
    .await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "nagad",
        Some(NAGAD_NUMBER),
        Some(NAGAD_API_KEY),
        Some(NAGAD_API_SECRET),
    )
    .await;
    // Stripe is fully configured (API credentials, so it IS offered
    // to payers) but carries no merchant_number — it must not appear
    // in receiving_numbers.
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "stripe",
        None,
        Some(STRIPE_API_KEY),
        Some(STRIPE_API_SECRET),
    )
    .await;

    let slug = create_link(&app, &token, &merchant_id).await;
    let body = get_public_link(&app, &slug).await;

    // Stripe really is an offered gateway on this link, so its
    // absence below is about the missing number, not about it not
    // being offered at all.
    let gateways = body["gateways"]
        .as_array()
        .expect("public view must list offered gateways");
    assert!(
        gateways.iter().any(|g| g == &json!("stripe")),
        "stripe must be offered (it is configured): {body}"
    );

    let numbers = body["receiving_numbers"]
        .as_array()
        .expect("public view must carry a receiving_numbers list");
    assert_eq!(
        numbers.len(),
        2,
        "exactly the two numbered gateways are listed: {body}"
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
    assert!(
        numbers.iter().all(|n| n["gateway"] != json!("stripe")),
        "a gateway without a merchant_number must be absent: {body}"
    );

    assert_no_secrets(&body, "public link view");
}

// ---------------------------------------------------------------------------
// (4) No configuration at all: checkout receiving_number is null
//     (present, not missing, not an empty string) and the public
//     link view's receiving_numbers is an empty list.
//
//     Implemented shape (verified against the Stream A handlers):
//     `checkout_details` always emits the `receiving_number` key
//     with a JSON null when no number is configured, and
//     `PublicLinkView.receiving_numbers` is a plain `Vec` that
//     serialises as `[]` when no offered gateway has a number.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_config_yields_null_receiving_number_and_empty_link_list() {
    let app = app();
    let token = register_and_login(&app, "recv-none@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;

    // Link view first, while the merchant has no gateway config at
    // all: the list is present and empty.
    let slug = create_link(&app, &token, &merchant_id).await;
    let body = get_public_link(&app, &slug).await;
    let object = body.as_object().expect("public view must be an object");
    assert!(
        object.contains_key("receiving_numbers"),
        "receiving_numbers key must be present: {body}"
    );
    assert_eq!(
        body["receiving_numbers"],
        json!([]),
        "with no gateway config the list must be empty: {body}"
    );

    // For the checkout side, create a payment while bKash is
    // configured, then delete the config entirely: at read time the
    // merchant has no config for the transaction's gateway.
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        Some(BKASH_API_KEY),
        Some(BKASH_API_SECRET),
    )
    .await;
    let transaction_id = create_payment(&app, &token, &merchant_id, "bkash").await;
    let (status, body) = send(
        &app,
        request(
            Method::DELETE,
            &format!("/api/v1/merchants/{merchant_id}/gateways/bkash"),
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert!(
        status == StatusCode::OK || status == StatusCode::NO_CONTENT,
        "delete bkash config: {status} {body}"
    );

    let body = get_checkout(&app, &transaction_id).await;
    let object = body.as_object().expect("checkout view must be an object");
    assert!(
        object.contains_key("receiving_number"),
        "receiving_number key must be present (not missing): {body}"
    );
    assert!(
        body["receiving_number"].is_null(),
        "receiving_number must be null with no config, not an empty \
         string or a stale number: {body}"
    );
}

// ---------------------------------------------------------------------------
// (5) The full public loop still settles with a receiving number
//     configured: public fetch → public pay → checkout submit →
//     device SMS → completed, with the receiving number visible on
//     the link view and on the checkout view throughout.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn public_link_flow_still_settles_with_receiving_number_configured() {
    let app = app();
    let token = register_and_login(&app, "recv-settle@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        Some(BKASH_API_KEY),
        Some(BKASH_API_SECRET),
    )
    .await;

    let slug = create_link(&app, &token, &merchant_id).await;

    // Public fetch: the receiving number is already visible here.
    let body = get_public_link(&app, &slug).await;
    let numbers = body["receiving_numbers"]
        .as_array()
        .expect("public view must carry receiving_numbers");
    assert!(
        numbers
            .iter()
            .any(|n| n == &json!({"gateway": "bkash", "number": BKASH_NUMBER})),
        "link view must list the bkash receiving number: {body}"
    );
    assert_no_secrets(&body, "public link view");

    // Public pay — creates the pending payment.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/payment-links/public/{slug}/pay"),
            None,
            None,
            Some(json!({
                "gateway": "bkash",
                "customer_phone": CUSTOMER_PHONE,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "public pay: {body}");
    assert_eq!(body["status"], json!("pending"));
    let transaction_id = body["transaction_id"]
        .as_str()
        .expect("pay response must carry the transaction id")
        .to_string();

    // The checkout view for the pending payment names the number
    // the payer must send to.
    let body = get_checkout(&app, &transaction_id).await;
    assert_eq!(body["status"], json!("pending"));
    assert_eq!(body["receiving_number"], json!(BKASH_NUMBER), "{body}");
    assert_no_secrets(&body, "pending checkout");

    // Checkout submit (sender + TrxID), as the hosted checkout does.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/checkout/{transaction_id}/submit"),
            None,
            None,
            Some(json!({
                "sender_number": CUSTOMER_PHONE,
                "provider_trx_id": "9HX7K2LMNP",
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "checkout submit: {body}");
    assert_eq!(body["transaction"]["status"], json!("processing"));

    // Device key + forwarded bKash SMS — the auto-approve step.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/api/v1/api-keys",
            Some(&token),
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
    let device_key = body["key"].as_str().expect("raw device key").to_string();

    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/api/v1/sms",
            None,
            Some(&device_key),
            Some(json!({
                "device_id": "receiving-number-android-device",
                "sender": "bKash",
                "body": BKASH_SMS_BODY,
                "received_at": "2026-11-26T03:16:00Z",
                "sim_slot": null,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sms intake: {body}");
    assert_eq!(body["outcome"], json!("verified"), "sms outcome: {body}");
    assert_eq!(body["transaction_id"], json!(transaction_id));

    // Settled — and the completed checkout still shows the number.
    let body = get_checkout(&app, &transaction_id).await;
    assert_eq!(body["status"], json!("completed"), "checkout: {body}");
    assert_eq!(body["receiving_number"], json!(BKASH_NUMBER), "{body}");

    let (status, body) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/payments/{transaction_id}"),
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get payment: {body}");
    assert_eq!(body["transaction"]["status"], json!("completed"));
}

// ---------------------------------------------------------------------------
// Coordinator fix round (A6 review): spec §2.2 predicate tests
// T3 / T5 / T6 / T8 — the shared helper must refuse credential
// gateways, disabled configs, blank numbers, and adapter-invalid
// identifiers on every public surface.
// ---------------------------------------------------------------------------

/// Fetch the public link view (no auth, as a payer's browser).
async fn get_link(app: &Router, slug: &str) -> Value {
    let (status, body) = send(
        app,
        request(
            Method::GET,
            &format!("/api/v1/payment-links/public/{slug}"),
            None,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "public link: {body}");
    body
}

/// T3 — a credential gateway's stored `merchant_number` is
/// meaningless and must never be exposed, even when set.
#[tokio::test]
async fn t3_credential_gateway_merchant_number_is_never_exposed() {
    let app = app();
    let token = register_and_login(&app, "t3-owner@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "stripe",
        Some("STRIPE-NUM-SENTINEL"),
        Some("sk_test_sentinel"),
        Some("whsec_sentinel"),
    )
    .await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        None,
        None,
    )
    .await;

    let slug = create_link(&app, &token, &merchant_id).await;
    let body = get_link(&app, &slug).await;
    assert!(
        !body.to_string().contains("STRIPE-NUM-SENTINEL"),
        "stripe merchant_number leaked on public link: {body}"
    );
    let entries = body["receiving_numbers"].as_array().expect("array");
    assert!(
        entries.iter().all(|e| e["gateway"] != json!("stripe")),
        "stripe must have no receiving_numbers entry: {body}"
    );
    assert!(
        entries
            .iter()
            .any(|e| e["gateway"] == json!("bkash") && e["number"] == json!(BKASH_NUMBER)),
        "bkash entry must remain: {body}"
    );

    // Same rule on the checkout list surface.
    let transaction_id = create_payment(&app, &token, &merchant_id, "bkash").await;
    let body = get_checkout(&app, &transaction_id).await;
    assert!(
        !body.to_string().contains("STRIPE-NUM-SENTINEL"),
        "stripe merchant_number leaked on checkout: {body}"
    );
}

/// T5 — a disabled gateway's number is not exposed, on the link
/// (including the sole-config fallback where every gateway is
/// offered) or as the checkout scalar.
#[tokio::test]
async fn t5_disabled_gateway_number_is_not_exposed() {
    let app = app();
    let token = register_and_login(&app, "t5-owner@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway_enabled(
        &app,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        false,
    )
    .await;

    let slug = create_link(&app, &token, &merchant_id).await;
    let body = get_link(&app, &slug).await;
    assert!(
        !body.to_string().contains(BKASH_NUMBER),
        "disabled bkash number leaked on public link: {body}"
    );

    // Checkout scalar: create while enabled, then disable.
    let app2 = crate::app();
    let token = register_and_login(&app2, "t5b-owner@example.com").await;
    let merchant_id = create_merchant(&app2, &token).await;
    configure_gateway(
        &app2,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        None,
        None,
    )
    .await;
    let transaction_id = create_payment(&app2, &token, &merchant_id, "bkash").await;
    let body = get_checkout(&app2, &transaction_id).await;
    assert_eq!(body["receiving_number"], json!(BKASH_NUMBER), "{body}");
    configure_gateway_enabled(
        &app2,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        false,
    )
    .await;
    let body = get_checkout(&app2, &transaction_id).await;
    assert!(
        body["receiving_number"].is_null(),
        "disabled gateway must null the checkout scalar: {body}"
    );
}

/// T6 — a blank (whitespace-only) stored number is exposed as
/// null / omitted, never as a whitespace string.
#[tokio::test]
async fn t6_blank_number_is_never_exposed() {
    let app = app();
    let token = register_and_login(&app, "t6-owner@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway(
        &app,
        &token,
        &merchant_id,
        "bkash",
        Some(BKASH_NUMBER),
        None,
        None,
    )
    .await;
    let transaction_id = create_payment(&app, &token, &merchant_id, "bkash").await;
    configure_gateway_enabled(&app, &token, &merchant_id, "bkash", Some("   "), true).await;
    let body = get_checkout(&app, &transaction_id).await;
    assert!(
        body["receiving_number"].is_null(),
        "blank number must read as null: {body}"
    );
    let entries = body["receiving_numbers"].as_array().expect("array");
    assert!(
        entries.iter().all(|e| e["gateway"] != json!("bkash")),
        "blank number must be omitted from the list: {body}"
    );
}

/// T8 — an adapter-invalid identifier (malformed UPI VPA) is not
/// exposed; the same config with a valid VPA is.
#[tokio::test]
async fn t8_adapter_invalid_identifier_is_not_exposed() {
    let app = app();
    let token = register_and_login(&app, "t8-owner@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway_enabled(&app, &token, &merchant_id, "upi", Some("shop"), true).await;
    let slug = create_link(&app, &token, &merchant_id).await;
    let body = get_link(&app, &slug).await;
    let entries = body["receiving_numbers"].as_array().expect("array");
    assert!(
        entries.iter().all(|e| e["gateway"] != json!("upi")),
        "invalid VPA must not be exposed: {body}"
    );

    configure_gateway_enabled(
        &app,
        &token,
        &merchant_id,
        "upi",
        Some("shop@okhdfcbank"),
        true,
    )
    .await;
    let body = get_link(&app, &slug).await;
    let entries = body["receiving_numbers"].as_array().expect("array");
    assert!(
        entries
            .iter()
            .any(|e| e["gateway"] == json!("upi") && e["number"] == json!("shop@okhdfcbank")),
        "valid VPA must be exposed: {body}"
    );
}
