//! End-to-end integration tests for the MiraclePay API (v0.0.1).
//!
//! Original clean-room tests. They drive the real router returned by
//! [`miraclepay_api::build_router`] in-process — no network port is
//! bound — by dispatching requests with `tower::ServiceExt::oneshot`
//! and reading the responses back as JSON.
//!
//! # Dependency note (important)
//!
//! `oneshot` comes from **tower 0.5** with the `util` feature, which
//! is the tower version axum 0.7.9 itself is built on. The `api`
//! crate's `Cargo.toml` currently declares `tower = "0.4"` (without
//! `util`, and tower is otherwise unused in the crate's sources), so
//! these tests require that line to be changed to:
//!
//! ```toml
//! tower = { version = "0.5", features = ["util"] }
//! ```
//!
//! (or the same line added under `[dev-dependencies]` with the stale
//! 0.4 entry removed). That change is intentionally NOT made here;
//! it is reported alongside this file.
//!
//! # Route / contract notes (adapted to the actual handlers)
//!
//! * The SMS intake route wired in `routes.rs` is `POST /api/v1/sms`
//!   — there is no `/api/v1/sms/incoming` route. Device auth is the
//!   raw API key in the `x-device-key` header, exactly as
//!   `handlers::sms` implements it.
//! * The follow-up read uses `GET /api/v1/payments/:id`, whose
//!   handler checks merchant ownership against the merchants map.
//!   (`GET /api/v1/transactions/:id` scopes by the token's
//!   `merchant_id` claim, which login tokens do not carry, so it
//!   would answer 403 for a normal merchant token.)
//! * SMS matching: `create_payment` stores the transaction as
//!   `Processing` with no `sender_number`, so a first bKash SMS that
//!   matches on amount + gateway scores as a *probable* match in
//!   `miraclepay-sms-verify` (amount 50 + gateway 20 = 70, below the
//!   80-point exact threshold) unless the provider transaction id
//!   also matches. The SMS test therefore accepts either designed
//!   outcome — `verified` (transaction then reads `completed`) or
//!   `probable` (transaction still reads `processing`) — and asserts
//!   that the read-back status is consistent with the outcome.

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use miraclepay_api::{build_router, AppState};

/// Signing secret for the throwaway in-memory state of each test.
const TEST_SECRET: &str = "integration-test-secret";

/// Merchant bKash number used in the gateway configuration and
/// expected back inside the payment instructions.
const MERCHANT_BKASH_NUMBER: &str = "01700000000";

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
// Harness helpers
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
                "name": "Integration Merchant",
                "email": email,
                "password": "password123",
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register: {body}");
    assert_eq!(body["email"], json!(email));
    assert_eq!(body["role"], json!("merchant"));

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
    let token = body["token"]
        .as_str()
        .expect("login response must carry a token")
        .to_string();
    assert!(!token.is_empty());
    token
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
                // The handler discards this and forces the owner to
                // the authenticated user; it is sent only because
                // the field is part of the request shape.
                "owner_user_id": "ignored-by-handler",
                "business_name": "Dhaka Integration Shop",
                "email": "shop@example.com",
                "phone": MERCHANT_BKASH_NUMBER,
                "country": "BD",
                "webhook_url": null,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create merchant: {body}");
    assert_eq!(body["business_name"], json!("Dhaka Integration Shop"));
    let merchant_id = body["id"]
        .as_str()
        .expect("merchant response must carry an id")
        .to_string();
    assert!(!merchant_id.is_empty());
    merchant_id
}

/// Configure the merchant's bKash gateway (enabled, with a merchant
/// number — the two things the bKash adapter requires to report
/// itself configured).
async fn configure_bkash(app: &Router, token: &str, merchant_id: &str) {
    let (status, body) = send(
        app,
        request(
            Method::PUT,
            &format!("/api/v1/merchants/{merchant_id}/gateways"),
            Some(token),
            None,
            Some(json!({
                "gateway": "bkash",
                "merchant_number": MERCHANT_BKASH_NUMBER,
                "api_key": null,
                "api_secret": null,
                "extra": {},
                "sandbox": true,
                "enabled": true,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "configure bkash: {body}");
    assert_eq!(body["gateway"], json!("bkash"));
    assert_eq!(
        body["configured"],
        json!(true),
        "bkash must be configured: {body}"
    );
}

/// Create a bKash payment of [`AMOUNT_MINOR`] for the merchant;
/// returns `(transaction_id, response_body)`.
async fn create_bkash_payment(app: &Router, token: &str, merchant_id: &str) -> (String, Value) {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/payments",
            Some(token),
            None,
            Some(json!({
                "merchant_id": merchant_id,
                "gateway": "bkash",
                "amount_minor": AMOUNT_MINOR,
                "currency": "BDT",
                "customer_email": "customer@example.com",
                "customer_phone": CUSTOMER_PHONE,
                "reference": "order-1001",
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create payment: {body}");
    let transaction_id = body["transaction"]["id"]
        .as_str()
        .expect("payment response must carry a transaction id")
        .to_string();
    assert!(!transaction_id.is_empty());
    (transaction_id, body)
}

/// Full happy-path setup shared by the payment tests: register,
/// login, merchant, bKash gateway. Returns `(token, merchant_id)`.
async fn setup_merchant_with_bkash(app: &Router, email: &str) -> (String, String) {
    let token = register_and_login(app, email).await;
    let merchant_id = create_merchant(app, &token).await;
    configure_bkash(app, &token, &merchant_id).await;
    (token, merchant_id)
}

// ---------------------------------------------------------------------------
// (1) Health
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_reports_ok_and_version_0_0_2() {
    let app = app();

    let (status, body) = send(&app, request(Method::GET, "/health", None, None, None)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], json!("ok"));
    assert_eq!(body["version"], json!("0.0.3"));
    assert_eq!(body["name"], json!("MiraclePay"));
}

// ---------------------------------------------------------------------------
// (2) Register -> login -> merchant -> bKash gateway -> payment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_login_merchant_gateway_payment_flow() {
    let app = app();
    let (token, merchant_id) = setup_merchant_with_bkash(&app, "flow-owner@example.com").await;

    let (transaction_id, body) = create_bkash_payment(&app, &token, &merchant_id).await;

    // The transaction is recorded and awaiting SMS verification.
    assert_eq!(body["transaction"]["id"], json!(transaction_id));
    assert_eq!(body["transaction"]["merchant_id"], json!(merchant_id));
    assert_eq!(body["transaction"]["gateway"], json!("bkash"));
    assert_eq!(body["transaction"]["status"], json!("processing"));
    assert_eq!(
        body["transaction"]["amount"]["amount_minor"],
        json!(AMOUNT_MINOR)
    );

    // A manual gateway returns instructions (not a redirect) naming
    // the gateway and the merchant number to send money to.
    let instructions = body["initiated"]["instructions"]
        .as_str()
        .expect("bkash payment must carry instructions");
    assert!(
        instructions.contains("bKash"),
        "instructions must name bKash: {instructions}"
    );
    assert!(
        instructions.contains(MERCHANT_BKASH_NUMBER),
        "instructions must name the merchant number: {instructions}"
    );
}

// ---------------------------------------------------------------------------
// (3) API key + forwarded bKash SMS settles / matches the payment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn api_key_device_sms_matches_pending_payment() {
    let app = app();
    let (token, merchant_id) = setup_merchant_with_bkash(&app, "sms-owner@example.com").await;
    let (transaction_id, _) = create_bkash_payment(&app, &token, &merchant_id).await;

    // Create an API key with the payments_write scope — the scope
    // the SMS intake requires of a device key.
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
    let device_key = body["key"]
        .as_str()
        .expect("api key creation must return the raw key once")
        .to_string();
    assert!(!device_key.is_empty());

    // Forward a realistic bKash confirmation SMS for the payment's
    // amount, authenticated only by the device key.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/api/v1/sms",
            None,
            Some(&device_key),
            Some(json!({
                "device_id": "integration-android-device",
                "sender": "bKash",
                "body": BKASH_SMS_BODY,
                "received_at": "2026-11-26T03:16:00Z",
                "sim_slot": null,
            })),
        ),
    )
    .await;

    // Both designed match outcomes are HTTP 200 and name the
    // transaction they matched (see the module note on scoring).
    assert_eq!(status, StatusCode::OK, "sms intake: {body}");
    let outcome = body["outcome"]
        .as_str()
        .expect("sms response must carry an outcome")
        .to_string();
    assert!(
        outcome == "verified" || outcome == "probable",
        "sms must match the pending payment, got outcome {outcome:?}: {body}"
    );
    assert_eq!(body["transaction_id"], json!(transaction_id));

    // Read the transaction back: a verified SMS completes it, a
    // probable match leaves it processing for follow-up. The status
    // must be consistent with the reported outcome.
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
    let tx_status = body["transaction"]["status"].as_str().unwrap_or_default();
    if outcome == "verified" {
        assert_eq!(
            tx_status, "completed",
            "verified sms must complete the payment"
        );
    } else {
        assert_eq!(
            tx_status, "processing",
            "probable match must not complete the payment automatically"
        );
    }
}

// ---------------------------------------------------------------------------
// (4) Unauthenticated access is rejected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unauthenticated_merchants_list_is_401() {
    let app = app();

    let (status, body) = send(
        &app,
        request(Method::GET, "/api/v1/merchants", None, None, None),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], json!("unauthorized"));
}

// ---------------------------------------------------------------------------
// Wave 24: security headers on every response
// ---------------------------------------------------------------------------

#[tokio::test]
async fn plugin_admin_and_verify_routes_are_registered() {
    let app = app();
    let token = register_and_login(&app, "plugin-routes@example.com").await;

    // A normal merchant token reaches the admin guard (403), rather
    // than falling through to the router's 404 fallback. This is a
    // regression test for handlers existing without route wiring.
    for (method, uri) in [
        (Method::GET, "/api/v1/admin/plugins".to_string()),
        (
            Method::POST,
            "/api/v1/admin/plugins/example/enable".to_string(),
        ),
        (
            Method::POST,
            "/api/v1/admin/plugins/example/disable".to_string(),
        ),
        (
            Method::GET,
            "/api/v1/admin/plugins/example/config".to_string(),
        ),
        (
            Method::PUT,
            "/api/v1/admin/plugins/example/config".to_string(),
        ),
        (
            Method::POST,
            "/api/v1/admin/plugins/example/invoke".to_string(),
        ),
        (
            Method::GET,
            "/api/v1/admin/plugins/example/logs".to_string(),
        ),
    ] {
        let body = if method == Method::PUT {
            Some(json!({}))
        } else if uri.ends_with("/invoke") {
            Some(json!({"operation": "ping", "args": {}}))
        } else {
            None
        };
        let (status, response) = send(&app, request(method, &uri, Some(&token), None, body)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {response}");
    }

    // The transaction verify route is also wired: a real token gets
    // the handler's transaction-level 404, not the router fallback.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/api/v1/transactions/txn_missing/verify",
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "verify route: {body}");
}

#[tokio::test]
async fn public_fx_rates_and_checkout_branding_are_served() {
    let app = app();
    let (token, merchant_id) = setup_merchant_with_bkash(&app, "branding@example.com").await;

    // FX rates are public, non-sensitive display data used by the
    // anonymous hosted pay/checkout pages.
    let (status, body) = send(
        &app,
        request(Method::GET, "/api/v1/fx/rates", None, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "public fx rates: {body}");
    assert_eq!(body["base"], json!("BDT"));
    assert!(body["rates"].is_array());

    // Round-trip the merchant settings with branding fields set.
    let (status, mut settings) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/merchants/{merchant_id}/settings"),
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get settings: {settings}");
    settings["brand_name"] = json!("Branded Shop");
    settings["brand_color"] = json!("#123abc");
    settings["logo_url"] = json!("https://example.com/logo.png");
    settings["checkout_message"] = json!("Thanks for shopping with us");
    let (status, body) = send(
        &app,
        request(
            Method::PUT,
            &format!("/api/v1/merchants/{merchant_id}/settings"),
            Some(&token),
            None,
            Some(settings),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "put settings: {body}");

    let (transaction_id, _) = create_bkash_payment(&app, &token, &merchant_id).await;
    let (status, body) = send(
        &app,
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
    assert_eq!(body["branding"]["display_name"], json!("Branded Shop"));
    assert_eq!(body["branding"]["brand_color"], json!("#123abc"));
    assert_eq!(
        body["branding"]["logo_url"],
        json!("https://example.com/logo.png")
    );
    assert_eq!(
        body["branding"]["checkout_message"],
        json!("Thanks for shopping with us")
    );
}

#[tokio::test]
async fn responses_carry_security_headers() {
    let app = app();
    let response = app
        .clone()
        .oneshot(request(Method::GET, "/health", None, None, None))
        .await
        .expect("router must answer");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
    assert!(headers
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("frame-ancestors 'none'"));
    assert!(headers.contains_key("strict-transport-security"));
    // CORS is off by default (no allowlist configured in tests):
    // no Access-Control-Allow-Origin may be emitted.
    assert!(!headers.contains_key("access-control-allow-origin"));
}
