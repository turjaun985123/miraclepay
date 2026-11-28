//! Stream D (v0.0.4) plugin-dispatch SAFETY tests — seat D5.
//!
//! Stream D wires two plugin kinds into the payment pipeline:
//!
//! * `notification` plugins — fire-and-forget `notify(event)` on
//!   `payment.completed` / `payment.failed`;
//! * `webhook_transformer` plugins — per-merchant opt-in transform
//!   of the outbound webhook payload at event-build time, bounded
//!   by the plugin's manifest timeout, falling back to the original
//!   payload on any failure.
//!
//! These tests prove the safety properties, in descending order of
//! importance: a hanging or throwing plugin must never block or
//! break settlement, a transformer failure must never corrupt the
//! webhook payload, and a transformer must never touch a merchant
//! who did not opt in. Happy-path delivery/transformation is
//! asserted too, because a safety test that passes vacuously when
//! dispatch is not wired at all is worthless: the recorder test
//! (test 2) is the canary — if `notify` is never dispatched, it
//! FAILS, and the vacuous passes of the fallback / no-opt-in tests
//! must be read alongside it.
//!
//! # Harness
//!
//! Fixture plugins live under `tests/fixtures/<id>/` (a
//! `plugin.toml` and a `main.js` per fixture), mirroring how
//! `plugins_pilot.rs` loads the real `plugins/` directory: each
//! test copies the fixtures it needs into a fresh temp dir and
//! runs the real [`miraclepay_api::plugins::scan_plugins`] over
//! it, so manifests, the confined engine, timeouts and the
//! state-backed KV are all the production code paths. Settlement
//! is driven through the real router exactly like
//! `integration.rs`: register, merchant, bKash gateway, payment,
//! sender and TrxID recorded (the checkout-submit data), then a
//! device-key bKash SMS, which settles deterministically
//! (`outcome == "verified"`, transaction `completed`, one
//! `payment.completed` webhook event stored in state).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use miraclepay_api::plugins;
use miraclepay_api::{build_router, AppState};

const TEST_SECRET: &str = "plugin-dispatch-test-secret";
const MERCHANT_BKASH_NUMBER: &str = "01700000000";
const CUSTOMER_PHONE: &str = "01712345678";
const AMOUNT_MINOR: i64 = 125_000;
const BKASH_SMS_BODY: &str = "You have received Tk 1,250.00 from 01712345678. \
    TrxID: 9HX7K2LMNP. Balance: Tk 5,430.50. Thank you for using bKash.";

/// Marker the `transform-marker` fixture stamps onto payloads.
const TRANSFORM_MARKER: &str = "transform-marker-v1";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Fixture loading (mirrors plugins_pilot.rs: scan a plugins dir)
// ---------------------------------------------------------------------------

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Copy the named fixture plugins into a fresh temp dir, scan them
/// into a fresh state with the real loader, and return
/// `(router, state, temp_dir)`. Asserts every fixture loaded
/// cleanly and is usable — a fixture that fails to load is a test
/// bug, not a dispatch result.
async fn app_with_fixtures(tag: &str, fixture_ids: &[&str]) -> (Router, AppState, PathBuf) {
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "mp-plugin-dispatch-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp plugins dir");
    for id in fixture_ids {
        let src = fixtures_root().join(id);
        let dst = dir.join(id);
        std::fs::create_dir_all(&dst).expect("create fixture dir");
        for file in ["plugin.toml", "main.js"] {
            std::fs::copy(src.join(file), dst.join(file))
                .unwrap_or_else(|e| panic!("copy fixture {id}/{file}: {e}"));
        }
    }

    let state = AppState::new(TEST_SECRET.to_string());
    plugins::scan_plugins(&state, &dir).await;
    {
        let loaded = state.plugins.read().await;
        for id in fixture_ids {
            let plugin = loaded
                .get(*id)
                .unwrap_or_else(|| panic!("fixture {id} was scanned"));
            assert!(
                plugin.load_error.is_none(),
                "fixture {id} must load cleanly: {:?}",
                plugin.load_error
            );
            assert!(plugin.enabled, "fixture {id} enabled by default");
            assert!(plugin.usable(), "fixture {id} usable");
        }
    }
    let router = build_router(state.clone());
    (router, state, dir)
}

fn cleanup(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

/// Read one value out of a plugin's KV via the shared state map
/// (keys are scoped `plugin_id \u{1} key` by the production store).
fn plugin_kv(state: &AppState, plugin_id: &str, key: &str) -> Option<Value> {
    let map = state.plugin_kv.lock().unwrap();
    map.get(&format!("{plugin_id}\u{1}{key}"))
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
}

fn plugin_kv_count(state: &AppState, plugin_id: &str, key: &str) -> i64 {
    plugin_kv(state, plugin_id, key)
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

/// Poll (50 ms interval, `deadline` total) until `check` holds.
/// Fire-and-forget delivery is asynchronous by design, so tests
/// wait on the observable effect with a deadline instead of a
/// fixed sleep.
async fn poll_until<F: Fn() -> bool>(deadline: Duration, check: F) -> bool {
    let started = Instant::now();
    loop {
        if check() {
            return true;
        }
        if started.elapsed() >= deadline {
            return check();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---------------------------------------------------------------------------
// Router harness (same shapes as integration.rs)
// ---------------------------------------------------------------------------

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

async fn register_and_login(app: &Router, email: &str) -> String {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/auth/register",
            None,
            None,
            Some(json!({
                "name": "Dispatch Merchant",
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
    body["token"].as_str().expect("login token").to_string()
}

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
                "business_name": "Dispatch Test Shop",
                "email": "shop@example.com",
                "phone": MERCHANT_BKASH_NUMBER,
                "country": "BD",
                "webhook_url": null,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create merchant: {body}");
    body["id"].as_str().expect("merchant id").to_string()
}

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
    assert_eq!(body["configured"], json!(true), "bkash configured: {body}");
}

async fn create_bkash_payment(app: &Router, token: &str, merchant_id: &str) -> String {
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
                "reference": "order-dispatch-1",
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create payment: {body}");
    body["transaction"]["id"]
        .as_str()
        .expect("transaction id")
        .to_string()
}

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
    body["key"].as_str().expect("raw device key").to_string()
}

/// Opt the merchant in to the given webhook-transformer plugin
/// via the merchant settings, round-tripped through the real
/// settings API. The field is
/// `MerchantSettings.webhook_transformer_plugin_id`.
async fn opt_in_transformer(app: &Router, token: &str, merchant_id: &str, plugin_id: &str) {
    let (status, mut settings) = send(
        app,
        request(
            Method::GET,
            &format!("/api/v1/merchants/{merchant_id}/settings"),
            Some(token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get settings: {settings}");
    settings["webhook_transformer_plugin_id"] = json!(plugin_id);
    let (status, body) = send(
        app,
        request(
            Method::PUT,
            &format!("/api/v1/merchants/{merchant_id}/settings"),
            Some(token),
            None,
            Some(settings),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "put settings (opt-in): {body}");
}

/// Full setup: user, merchant, bKash gateway, one pending payment,
/// one device key. Returns `(token, merchant_id, tx_id, device_key)`.
async fn setup_pending_payment(app: &Router, email: &str) -> (String, String, String, String) {
    let token = register_and_login(app, email).await;
    let merchant_id = create_merchant(app, &token).await;
    configure_bkash(app, &token, &merchant_id).await;
    let tx_id = create_bkash_payment(app, &token, &merchant_id).await;
    let device_key = create_device_key(app, &token, &merchant_id).await;
    (token, merchant_id, tx_id, device_key)
}

/// Settle the pending payment through the SMS loop. The hosted
/// checkout's submit step records the payer's sender number and
/// the claimed provider TrxID on the transaction; this does the
/// same directly in state (checkout submit itself only accepts
/// link-created `pending` transactions, while API-created ones are
/// already `processing`), then forwards the bKash confirmation
/// SMS. Returns `(sms_status, sms_body, settle_elapsed)` where the
/// elapsed time covers ONLY the SMS settle request — the request a
/// hanging plugin must not be able to stall.
async fn settle_via_sms(
    app: &Router,
    state: &AppState,
    tx_id: &str,
    device_key: &str,
) -> (StatusCode, Value, Duration) {
    {
        let mut transactions = state.transactions.write().await;
        let tx = transactions
            .get_mut(tx_id)
            .expect("transaction must exist in state");
        tx.sender_number = Some(CUSTOMER_PHONE.to_string());
        tx.provider_trx_id = Some("9HX7K2LMNP".to_string());
    }

    let started = Instant::now();
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/sms",
            None,
            Some(device_key),
            Some(json!({
                "device_id": "dispatch-test-device",
                "sender": "bKash",
                "body": BKASH_SMS_BODY,
                "received_at": "2026-11-26T03:16:00Z",
                "sim_slot": null,
            })),
        ),
    )
    .await;
    (status, body, started.elapsed())
}

/// Assert the SMS settle completed the payment end to end.
async fn assert_settled(app: &Router, token: &str, tx_id: &str, sms_body: &Value) {
    assert_eq!(
        sms_body["outcome"],
        json!("verified"),
        "sms outcome: {sms_body}"
    );
    assert_eq!(sms_body["transaction_id"], json!(tx_id));
    let (status, body) = send(
        app,
        request(
            Method::GET,
            &format!("/api/v1/payments/{tx_id}"),
            Some(token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get payment: {body}");
    assert_eq!(
        body["transaction"]["status"],
        json!("completed"),
        "payment must be completed: {body}"
    );
}

/// The stored `payment.completed` webhook event payload for `tx_id`.
async fn completed_event_payload(state: &AppState, tx_id: &str) -> Value {
    let events = state.webhook_events.read().await;
    let event = events
        .iter()
        .find(|e| {
            e.event_type == "payment.completed" && e.payload["transaction_id"] == json!(tx_id)
        })
        .unwrap_or_else(|| {
            panic!(
                "a payment.completed webhook event for {tx_id} must be stored; \
                 stored event types: {:?}",
                events.iter().map(|e| &e.event_type).collect::<Vec<_>>()
            )
        });
    event.payload.clone()
}

/// Assert a payload is the untouched original settlement payload:
/// right transaction, right amount, and no transformer marker
/// anywhere in it.
fn assert_original_payload(payload: &Value, tx_id: &str) {
    assert_eq!(
        payload["transaction_id"],
        json!(tx_id),
        "payload transaction: {payload}"
    );
    assert_eq!(
        payload["amount_minor"],
        json!(AMOUNT_MINOR),
        "payload amount: {payload}"
    );
    assert!(
        !payload.to_string().contains(TRANSFORM_MARKER),
        "payload must NOT carry the transformer marker: {payload}"
    );
}

// ---------------------------------------------------------------------------
// (1) A hanging notification plugin cannot block settlement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hanging_notification_plugin_cannot_block_settlement() {
    let (app, state, dir) = app_with_fixtures("hang", &["notify-hang"]).await;
    let (token, _merchant, tx_id, device_key) =
        setup_pending_payment(&app, "hang-owner@example.com").await;

    let (status, body, elapsed) = settle_via_sms(&app, &state, &tx_id, &device_key).await;

    assert_eq!(status, StatusCode::OK, "sms settle: {body}");
    eprintln!("[d5 timing] hanging notify: settle request elapsed = {elapsed:?}");
    // Timing evidence: notify-hang loops forever with a 300 ms
    // manifest timeout. Fire-and-forget dispatch returns at once;
    // even a (wrong) awaited-but-timeout-bounded dispatch would
    // return in ~300 ms. 10 s is the generous bound that separates
    // "bounded" from "settlement waits on a dead plugin".
    assert!(
        elapsed < Duration::from_secs(10),
        "settle request took {elapsed:?} — a hanging notification plugin blocked settlement"
    );
    assert_settled(&app, &token, &tx_id, &body).await;
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// (2) Notification delivery happens (the dispatch canary)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn notification_plugin_records_payment_completed() {
    let (app, state, dir) = app_with_fixtures("recorder", &["notify-recorder"]).await;
    let (token, _merchant, tx_id, device_key) =
        setup_pending_payment(&app, "recorder-owner@example.com").await;

    let (status, body, _elapsed) = settle_via_sms(&app, &state, &tx_id, &device_key).await;
    assert_eq!(status, StatusCode::OK, "sms settle: {body}");
    assert_settled(&app, &token, &tx_id, &body).await;

    // Fire-and-forget: allow a short poll (2 s deadline, 50 ms
    // interval) for the background delivery to land in the
    // plugin's KV. If notify is never dispatched this FAILS — by
    // design; it is the canary for the whole stream.
    let state_ref = &state;
    let delivered = poll_until(Duration::from_secs(2), || {
        plugin_kv_count(state_ref, "notify-recorder", "count:payment.completed") >= 1
    })
    .await;
    assert!(
        delivered,
        "notify-recorder must record a payment.completed within 2s of settlement; \
         kv count:payment.completed = {}, count:total = {}",
        plugin_kv_count(&state, "notify-recorder", "count:payment.completed"),
        plugin_kv_count(&state, "notify-recorder", "count:total"),
    );
    let last = plugin_kv(&state, "notify-recorder", "last:payment.completed")
        .expect("recorder stored the last payment.completed event");
    assert!(
        last.to_string().contains(&tx_id),
        "recorded event must be about the settled transaction {tx_id}: {last}"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// (3) Transformer applies for an opted-in merchant
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transformer_applies_for_opted_in_merchant() {
    let (app, state, dir) = app_with_fixtures("marker", &["transform-marker"]).await;
    let (token, merchant_id, tx_id, device_key) =
        setup_pending_payment(&app, "marker-owner@example.com").await;
    opt_in_transformer(&app, &token, &merchant_id, "transform-marker").await;

    let (status, body, _elapsed) = settle_via_sms(&app, &state, &tx_id, &device_key).await;
    assert_eq!(status, StatusCode::OK, "sms settle: {body}");
    assert_settled(&app, &token, &tx_id, &body).await;

    let payload = completed_event_payload(&state, &tx_id).await;
    assert!(
        payload.to_string().contains(TRANSFORM_MARKER),
        "opted-in merchant's webhook payload must carry the transformer marker \
         {TRANSFORM_MARKER:?}: {payload}"
    );
    // The transform must ADD the marker, not replace the payload.
    assert_eq!(
        payload["transaction_id"],
        json!(tx_id),
        "payload: {payload}"
    );
    assert_eq!(
        payload["amount_minor"],
        json!(AMOUNT_MINOR),
        "payload: {payload}"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// (4a) Transformer that throws → original payload, settlement fine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transformer_throw_falls_back_to_original_payload() {
    let (app, state, dir) = app_with_fixtures("tthrow", &["transform-throw"]).await;
    let (token, merchant_id, tx_id, device_key) =
        setup_pending_payment(&app, "tthrow-owner@example.com").await;
    opt_in_transformer(&app, &token, &merchant_id, "transform-throw").await;

    let (status, body, _elapsed) = settle_via_sms(&app, &state, &tx_id, &device_key).await;
    assert_eq!(status, StatusCode::OK, "sms settle: {body}");
    assert_settled(&app, &token, &tx_id, &body).await;

    let payload = completed_event_payload(&state, &tx_id).await;
    assert_original_payload(&payload, &tx_id);
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// (4b) Transformer that hangs past its timeout → original payload
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transformer_hang_falls_back_to_original_payload() {
    let (app, state, dir) = app_with_fixtures("thang", &["transform-hang"]).await;
    let (token, merchant_id, tx_id, device_key) =
        setup_pending_payment(&app, "thang-owner@example.com").await;
    opt_in_transformer(&app, &token, &merchant_id, "transform-hang").await;

    let (status, body, elapsed) = settle_via_sms(&app, &state, &tx_id, &device_key).await;
    assert_eq!(status, StatusCode::OK, "sms settle: {body}");
    eprintln!("[d5 timing] hanging transformer: settle request elapsed = {elapsed:?}");
    // The transform runs at event-build time on the settle path, so
    // its 300 ms manifest timeout bounds this request; 10 s is the
    // generous bound separating "bounded + fallback" from "the
    // settle waited on a dead plugin".
    assert!(
        elapsed < Duration::from_secs(10),
        "settle request took {elapsed:?} — a hanging transformer blocked settlement"
    );
    assert_settled(&app, &token, &tx_id, &body).await;

    let payload = completed_event_payload(&state, &tx_id).await;
    assert_original_payload(&payload, &tx_id);
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// (5a) No opt-in → no transform, even with a transformer loaded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_opt_in_no_transform() {
    let (app, state, dir) = app_with_fixtures("noopt", &["transform-marker"]).await;
    let (token, _merchant, tx_id, device_key) =
        setup_pending_payment(&app, "noopt-owner@example.com").await;
    // Deliberately NO opt_in_transformer call for this merchant.

    let (status, body, _elapsed) = settle_via_sms(&app, &state, &tx_id, &device_key).await;
    assert_eq!(status, StatusCode::OK, "sms settle: {body}");
    assert_settled(&app, &token, &tx_id, &body).await;

    let payload = completed_event_payload(&state, &tx_id).await;
    assert_original_payload(&payload, &tx_id);
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// (5b) Notification plugin that throws → settlement unaffected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn throwing_notification_plugin_does_not_affect_settlement() {
    let (app, state, dir) = app_with_fixtures("nthrow", &["notify-throw"]).await;
    let (token, _merchant, tx_id, device_key) =
        setup_pending_payment(&app, "nthrow-owner@example.com").await;

    let (status, body, elapsed) = settle_via_sms(&app, &state, &tx_id, &device_key).await;
    assert_eq!(status, StatusCode::OK, "sms settle: {body}");
    assert!(
        elapsed < Duration::from_secs(10),
        "settle request took {elapsed:?} with a throwing notification plugin"
    );
    assert_settled(&app, &token, &tx_id, &body).await;

    // The webhook pipeline itself is untouched by the plugin's
    // failure: the original event is stored, marker-free.
    let payload = completed_event_payload(&state, &tx_id).await;
    assert_eq!(
        payload["transaction_id"],
        json!(tx_id),
        "payload: {payload}"
    );
    assert_eq!(
        payload["amount_minor"],
        json!(AMOUNT_MINOR),
        "payload: {payload}"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// Coordinator fix round (D6 F5): spec T-7 — the F1 dispatch site.
// When a plugin gateway's verify reports the payment unverified,
// notification plugins receive `payment.failed` with reason
// "provider_unverified", and the transaction status is untouched.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unverified_plugin_verify_fires_payment_failed_notification() {
    let (app, state, dir) =
        app_with_fixtures("verifyfail", &["notify-recorder", "verify-fail"]).await;
    let token = register_and_login(&app, "verifyfail-owner@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;

    // Create a payment on the fixture gateway (plugin lane).
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/api/v1/payments",
            Some(&token),
            None,
            Some(json!({
                "merchant_id": merchant_id,
                "gateway": "verify_fail",
                "amount_minor": 125_000,
                "currency": "BDT",
                "customer_email": "customer@example.com",
                "customer_phone": "01712345678",
                "reference": "order-verify-fail",
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create via plugin gateway: {body}"
    );
    let tx_id = body["transaction"]["id"]
        .as_str()
        .expect("transaction id")
        .to_string();
    let status_before = body["transaction"]["status"].clone();

    // The merchant asks the platform to verify: the plugin says no.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/transactions/{tx_id}/verify"),
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "verify call: {body}");
    assert_ne!(
        body["status"],
        json!("completed"),
        "unverified payment must not complete: {body}"
    );

    // F1: payment.failed is dispatched, fire-and-forget — poll.
    let state_ref = &state;
    let delivered = poll_until(Duration::from_secs(2), || {
        plugin_kv_count(state_ref, "notify-recorder", "count:payment.failed") >= 1
    })
    .await;
    assert!(
        delivered,
        "notify-recorder must record a payment.failed within 2s; \
         kv count:payment.failed = {}, count:total = {}",
        plugin_kv_count(&state, "notify-recorder", "count:payment.failed"),
        plugin_kv_count(&state, "notify-recorder", "count:total"),
    );
    let last = plugin_kv(&state, "notify-recorder", "last:payment.failed")
        .expect("recorder stored the last payment.failed event");
    assert!(
        last.to_string().contains(&tx_id),
        "payment.failed must be about the transaction {tx_id}: {last}"
    );
    assert!(
        last.to_string().contains("provider_unverified"),
        "payment.failed must carry the provider_unverified reason: {last}"
    );

    // No status mutation at F1: the transaction is exactly as
    // created (still awaiting a real verification).
    let (status, body) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/payments/{tx_id}"),
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get payment: {body}");
    assert_eq!(body["transaction"]["status"], status_before, "{body}");
    cleanup(&dir);
}
