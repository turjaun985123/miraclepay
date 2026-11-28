//! Adversarial edge tests for the v0.0.4 Stream B gateway picker on
//! `/checkout` (seat B5 — edges the B4 happy-path tests do not cover).
//!
//! Contract under test (Stream B spec):
//!
//! * `GET /api/v1/checkout/:id` gains `gateways` (the offered set)
//!   and `receiving_numbers` (per-gateway merchant numbers).
//! * `POST /api/v1/checkout/:id/submit` gains an OPTIONAL `gateway`
//!   field: when present it must be validated against the offered
//!   set, and the (canonical) value is stored on the still-`Pending`
//!   transaction, driving settlement.
//!
//! Cases:
//!
//! 1. Case / whitespace normalisation of the submitted gateway.
//! 2. A gateway offered only via an enabled PLUGIN (no native
//!    config) — offered / accepted, or not? `create_payment` accepts
//!    plugin gateways, so checkout must not silently diverge.
//! 3. Merchant with NO gateway configs (offered = all supported):
//!    a supported-but-unconfigured gateway at submit, and what
//!    settlement does with it afterwards.
//! 4. Replay / mismatch safety (FINDING, pinned): a bKash-format SMS
//!    carrying the submitted TrxID settles a transaction whose
//!    stored gateway was switched to `nagad` — the SMS matcher
//!    awards gateway match only +20 bonus points and never vetoes
//!    on gateway mismatch, so TrxID (100) + amount (50) + sender
//!    (30) reach the exact threshold without it.
//! 5. Amount binding on a switched-gateway transaction: right
//!    TrxID, wrong amount must NOT settle.
//!
//! The harness mirrors `receiving_number.rs`: the real router from
//! [`miraclepay_api::build_router`] driven in-process via
//! `tower::ServiceExt::oneshot` over fresh in-memory state.

use std::path::PathBuf;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use miraclepay_api::plugins;
use miraclepay_api::{build_router, AppState};

const TEST_SECRET: &str = "checkout-gateway-edges-test-secret";

const BKASH_NUMBER: &str = "01711111111";
const NAGAD_NUMBER: &str = "01822222222";
const CUSTOMER_PHONE: &str = "01712345678";
/// 1,250.00 BDT in minor units.
const AMOUNT_MINOR: i64 = 125_000;

/// TrxID the payer submits at checkout in the mismatch cases; the
/// bKash SMS below carries exactly this id.
const SHARED_TRX: &str = "9HX7K2LMNP";
/// TrxID of the Nagad template SMS.
const NAGAD_TRX: &str = "NAG7X2K9P1";

fn bkash_sms_body(amount: &str, trx: &str) -> String {
    format!(
        "You have received Tk {amount} from {CUSTOMER_PHONE}. \
         TrxID: {trx}. Balance: Tk 5,430.50. Thank you for using bKash."
    )
}

const NAGAD_SMS_BODY: &str = "You have received Tk 1,250.00 from 01712-345678. \
    Trx ID: NAG7X2K9P1. Your new balance is Tk 5,430.50. - Nagad";

// ---------------------------------------------------------------------------
// Harness helpers (mirroring receiving_number.rs)
// ---------------------------------------------------------------------------

fn app() -> Router {
    build_router(AppState::new(TEST_SECRET.to_string()))
}

/// Router over a state whose plugin directory has been scanned, so
/// the pilot plugins (incl. the `manual_bank_transfer` gateway
/// plugin) are loaded and enabled.
async fn app_with_plugins() -> Router {
    let state = AppState::new(TEST_SECRET.to_string());
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    plugins::scan_plugins(&state, &dir).await;
    build_router(state)
}

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
                "name": "Gateway Edge Merchant",
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
    body["token"].as_str().expect("token").to_string()
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
                "business_name": "Gateway Edge Shop",
                "email": "shop@example.com",
                "phone": BKASH_NUMBER,
                "country": "BD",
                "webhook_url": null,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create merchant: {body}");
    body["id"].as_str().expect("merchant id").to_string()
}

async fn configure_gateway(
    app: &Router,
    token: &str,
    merchant_id: &str,
    gateway: &str,
    merchant_number: Option<&str>,
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
                "api_key": format!("{gateway}-key"),
                "api_secret": format!("{gateway}-secret"),
                "extra": {},
                "sandbox": true,
                "enabled": true,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "configure {gateway}: {body}");
}

/// Create a `Pending` transaction the way a payer does: through a
/// fixed-amount payment link, initially on `gateway`.
async fn pending_via_link(app: &Router, token: &str, merchant_id: &str, gateway: &str) -> String {
    let (status, body) = send(
        app,
        request(
            Method::POST,
            "/api/v1/payment-links",
            Some(token),
            None,
            Some(json!({
                "merchant_id": merchant_id,
                "title": "Gateway Edge Widget",
                "description": "edge-case widget",
                "amount_minor": AMOUNT_MINOR,
                "currency": "BDT",
                "collect_phone": true,
                "collect_email": false,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create link: {body}");
    let slug = body["slug"].as_str().expect("slug").to_string();

    let (status, body) = send(
        app,
        request(
            Method::POST,
            &format!("/api/v1/payment-links/public/{slug}/pay"),
            None,
            None,
            Some(json!({ "gateway": gateway, "customer_phone": CUSTOMER_PHONE })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "public pay: {body}");
    assert_eq!(body["status"], json!("pending"));
    body["transaction_id"]
        .as_str()
        .expect("transaction id")
        .to_string()
}

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

/// Submit checkout. `gateway: None` omits the field entirely (the
/// pre-Stream-B request shape).
async fn submit(
    app: &Router,
    transaction_id: &str,
    trx: &str,
    gateway: Option<&str>,
) -> (StatusCode, Value) {
    let mut body = json!({
        "sender_number": CUSTOMER_PHONE,
        "provider_trx_id": trx,
    });
    if let Some(gateway) = gateway {
        body["gateway"] = json!(gateway);
    }
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

async fn device_key(app: &Router, token: &str, merchant_id: &str) -> String {
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

async fn forward_sms(app: &Router, key: &str, sender: &str, body: &str) -> (StatusCode, Value) {
    send(
        app,
        request(
            Method::POST,
            "/api/v1/sms",
            None,
            Some(key),
            Some(json!({
                "device_id": "gateway-edge-android-device",
                "sender": sender,
                "body": body,
                "received_at": "2026-11-26T03:16:00Z",
                "sim_slot": null,
            })),
        ),
    )
    .await
}

/// Merchant configured for both bKash and Nagad, with a pending
/// bKash transaction. Returns (token, merchant_id, tx_id).
async fn bkash_nagad_pending(app: &Router, email: &str) -> (String, String, String) {
    let token = register_and_login(app, email).await;
    let merchant_id = create_merchant(app, &token).await;
    configure_gateway(app, &token, &merchant_id, "bkash", Some(BKASH_NUMBER)).await;
    configure_gateway(app, &token, &merchant_id, "nagad", Some(NAGAD_NUMBER)).await;
    let tx = pending_via_link(app, &token, &merchant_id, "bkash").await;
    (token, merchant_id, tx)
}

// ---------------------------------------------------------------------------
// (1) Case / whitespace: " BKASH " and "Bkash" must be normalised
//     to the canonical id and stored as such — mirroring
//     `GatewayId::from_str` / `registry.get_by_str`, which trim and
//     case-fold, and `create_payment`, which stores
//     `req.gateway.trim().to_ascii_lowercase()`.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submit_gateway_with_whitespace_and_case_is_normalised() {
    let app = app();
    let (_t, _m, tx) = bkash_nagad_pending(&app, "edge-case1a@example.com").await;

    let (status, body) = submit(&app, &tx, SHARED_TRX, Some(" BKASH ")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "FINDING if failing: submit rejects a padded gateway spelling \
         that the native registry would accept: {body}"
    );
    assert_eq!(
        body["transaction"]["gateway"],
        json!("bkash"),
        "stored gateway must be the canonical id, not the raw spelling: {body}"
    );
    let view = get_checkout(&app, &tx).await;
    assert_eq!(view["gateway"], json!("bkash"), "{view}");
    assert_eq!(view["receiving_number"], json!(BKASH_NUMBER), "{view}");
}

#[tokio::test]
async fn submit_gateway_mixed_case_switch_is_stored_canonical() {
    let app = app();
    let (_t, _m, tx) = bkash_nagad_pending(&app, "edge-case1b@example.com").await;

    // Switch bKash -> Nagad with a mixed-case, padded spelling.
    let (status, body) = submit(&app, &tx, NAGAD_TRX, Some(" Nagad ")).await;
    assert_eq!(status, StatusCode::OK, "submit switch: {body}");
    assert_eq!(
        body["transaction"]["gateway"],
        json!("nagad"),
        "FINDING if failing: switched gateway not stored canonical: {body}"
    );
    let view = get_checkout(&app, &tx).await;
    assert_eq!(view["gateway"], json!("nagad"), "{view}");
    assert_eq!(
        view["receiving_number"],
        json!(NAGAD_NUMBER),
        "receiving number must follow the switched gateway: {view}"
    );
}

#[tokio::test]
async fn submit_gateway_not_offered_is_rejected_and_tx_unchanged() {
    let app = app();
    let (_t, _m, tx) = bkash_nagad_pending(&app, "edge-case1c@example.com").await;

    // Stripe is a supported gateway but NOT offered for this
    // merchant (only bkash + nagad configured).
    let (status, body) = submit(&app, &tx, SHARED_TRX, Some("stripe")).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "FINDING if failing: submit accepted a gateway outside the \
         offered set: {body}"
    );
    // A rejected submit must not consume the transaction: it stays
    // Pending on its original gateway and can still be submitted.
    let view = get_checkout(&app, &tx).await;
    assert_eq!(view["status"], json!("pending"), "{view}");
    assert_eq!(view["gateway"], json!("bkash"), "{view}");
    let (status, _body) = submit(&app, &tx, SHARED_TRX, Some("bkash")).await;
    assert_eq!(status, StatusCode::OK, "tx must remain submittable");

    // A garbage id is rejected the same way (fresh transaction).
    let (_t2, _m2, tx2) = bkash_nagad_pending(&app, "edge-case1d@example.com").await;
    let (status, body) = submit(&app, &tx2, SHARED_TRX, Some("bkash2")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "garbage gateway: {body}");
}

// ---------------------------------------------------------------------------
// (2) Plugin-only gateway: `manual_bank_transfer` is provided by
//     the enabled manual-bank-transfer pilot plugin and has NO
//     native adapter/config. `create_payment` accepts it through
//     the plugin lane (payments.rs: plugin_provides_gateway
//     fall-through). Checkout must behave consistently: the id
//     should appear in the offered set and be accepted at submit.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn plugin_only_gateway_is_offered_and_accepted_at_checkout() {
    let app = app_with_plugins().await;
    let token = register_and_login(&app, "edge-plugin@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway(&app, &token, &merchant_id, "bkash", Some(BKASH_NUMBER)).await;
    let tx = pending_via_link(&app, &token, &merchant_id, "bkash").await;

    let view = get_checkout(&app, &tx).await;
    let offered: Vec<&str> = view["gateways"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert!(
        offered.contains(&"manual_bank_transfer"),
        "FINDING if failing: plugin-provided gateway missing from \
         checkout offered set {offered:?} (create_payment accepts it \
         via the plugin lane — inconsistent): {view}"
    );

    let (status, body) = submit(&app, &tx, SHARED_TRX, Some("manual_bank_transfer")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "FINDING if failing: submit rejects the plugin gateway that \
         create_payment accepts: {body}"
    );
    assert_eq!(
        body["transaction"]["gateway"],
        json!("manual_bank_transfer")
    );
}

/// Consistency pin (v0.0.4 FINDING-1, fixed at integration):
/// plugin-provided gateways are folded into the offered set on
/// BOTH public surfaces (`checkout_details` here, the public link
/// view likewise), so what `submit_checkout` accepts is exactly
/// what the picker can offer. Before the fix this test pinned the
/// inconsistent behaviour (accepted at submit, absent from
/// `gateways`); it now pins the consistent one.
#[tokio::test]
async fn pin_plugin_gateway_is_offered_and_accepted_consistently() {
    let app = app_with_plugins().await;
    let token = register_and_login(&app, "edge-plugin-pin@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    configure_gateway(&app, &token, &merchant_id, "bkash", Some(BKASH_NUMBER)).await;
    let tx = pending_via_link(&app, &token, &merchant_id, "bkash").await;

    let view = get_checkout(&app, &tx).await;
    let offered: Vec<&str> = view["gateways"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert!(
        offered.contains(&"manual_bank_transfer"),
        "plugin gateway must be offered once FINDING-1 is fixed: {offered:?}"
    );

    let (status, body) = submit(&app, &tx, SHARED_TRX, Some("manual_bank_transfer")).await;
    assert_eq!(status, StatusCode::OK, "plugin submit: {body}");
    assert_eq!(
        body["transaction"]["gateway"],
        json!("manual_bank_transfer")
    );
}

// ---------------------------------------------------------------------------
// (3) No gateway configs at all: offered = every supported gateway
//     (the `registry_for` / public-link semantics). Submitting a
//     supported-but-unconfigured gateway is therefore accepted —
//     and settlement afterwards is purely SMS-driven: the matcher
//     never consults gateway configs, so a matching Nagad SMS
//     settles the switched transaction even though the merchant
//     has no Nagad account on file (and `receiving_numbers` gave
//     the payer no number to send to). Pinned as actual semantics.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unconfigured_merchant_offered_all_and_unconfigured_gateway_settles_by_sms() {
    let app = app();
    let token = register_and_login(&app, "edge-noconfig@example.com").await;
    let merchant_id = create_merchant(&app, &token).await;
    // Deliberately NO configure_gateway calls.
    let tx = pending_via_link(&app, &token, &merchant_id, "bkash").await;

    let view = get_checkout(&app, &tx).await;
    let offered: Vec<&str> = view["gateways"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert!(
        offered.contains(&"bkash") && offered.contains(&"nagad") && offered.contains(&"stripe"),
        "with no configs the offered set must be all supported gateways: {offered:?}"
    );
    assert_eq!(
        view["receiving_numbers"],
        json!([]),
        "no configs => no receiving numbers to show the payer: {view}"
    );

    // Supported but unconfigured: accepted at submit.
    let (status, body) = submit(&app, &tx, NAGAD_TRX, Some("nagad")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "supported-but-unconfigured gateway must be accepted when \
         the merchant configured nothing: {body}"
    );
    assert_eq!(body["transaction"]["gateway"], json!("nagad"));

    // Settlement is config-independent: a matching Nagad SMS
    // completes the transaction despite there being no Nagad
    // config (and no receiving number) for this merchant at all.
    let key = device_key(&app, &token, &merchant_id).await;
    let (status, body) = forward_sms(&app, &key, "Nagad", NAGAD_SMS_BODY).await;
    assert_eq!(status, StatusCode::OK, "sms intake: {body}");
    assert_eq!(
        body["outcome"],
        json!("verified"),
        "SMS matching does not consult gateway configs: {body}"
    );
    assert_eq!(body["transaction_id"], json!(tx));
    let view = get_checkout(&app, &tx).await;
    assert_eq!(view["status"], json!("completed"), "{view}");
}

// ---------------------------------------------------------------------------
// (4) Cross-channel settlement — FINDING-2, FIXED in v0.0.4 at
//     integration (matcher channel-binding guard): the payer
//     switches the transaction to `nagad` at submit and reports
//     TrxID X; a bKash-format SMS carrying TrxID X, the right
//     amount and the right sender must NOT auto-settle the nagad
//     transaction. Before the fix the matcher's gateway signal
//     was a +20 bonus, never a veto, so 100 (TrxID) + 50 (amount)
//     + 30 (sender) = 180 >= 80 settled it — the v0.0.1
//     wrong-channel bug class. Now a known channel mismatch caps
//     the match at Probable: reported for a human, the TrxID is
//     not reserved, and the transaction stays `processing`.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn finding_bkash_sms_settles_nagad_switched_transaction() {
    let app = app();
    let (token, merchant_id, tx) = bkash_nagad_pending(&app, "edge-mismatch@example.com").await;

    let (status, body) = submit(&app, &tx, SHARED_TRX, Some("nagad")).await;
    assert_eq!(status, StatusCode::OK, "submit switch to nagad: {body}");
    assert_eq!(body["transaction"]["gateway"], json!("nagad"));

    let key = device_key(&app, &token, &merchant_id).await;
    let (status, body) =
        forward_sms(&app, &key, "bKash", &bkash_sms_body("1,250.00", SHARED_TRX)).await;
    // Fixed behaviour: cross-channel evidence is probable at
    // best — never an automatic verified settle.
    assert_eq!(status, StatusCode::OK, "sms intake: {body}");
    assert_eq!(
        body["outcome"],
        json!("probable"),
        "bKash SMS against a nagad transaction must be probable, \
         not verified: {body}"
    );
    assert_eq!(body["transaction_id"], json!(tx));
    let view = get_checkout(&app, &tx).await;
    assert_eq!(
        view["status"],
        json!("processing"),
        "transaction must remain unsettled on cross-channel evidence: {view}"
    );
    assert_eq!(view["gateway"], json!("nagad"), "{view}");
}

// ---------------------------------------------------------------------------
// (5) Amount binding on a switched transaction: right TrxID and
//     right channel shape, WRONG amount — must not settle. The
//     matcher's amount-contradiction guard (Wave 8) blocks Exact
//     and Probable requires an amount match, so the SMS is
//     unmatched and the transaction stays `processing`.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn wrong_amount_sms_does_not_settle_switched_transaction() {
    let app = app();
    let (token, merchant_id, tx) = bkash_nagad_pending(&app, "edge-amount@example.com").await;

    let (status, body) = submit(&app, &tx, SHARED_TRX, Some("nagad")).await;
    assert_eq!(status, StatusCode::OK, "submit switch to nagad: {body}");

    let key = device_key(&app, &token, &merchant_id).await;
    // Tk 999.00 instead of Tk 1,250.00, same TrxID, bKash channel.
    let (status, body) =
        forward_sms(&app, &key, "bKash", &bkash_sms_body("999.00", SHARED_TRX)).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "wrong-amount SMS must settle nothing (202 unmatched): {body}"
    );
    assert_eq!(body["outcome"], json!("unmatched"), "{body}");

    // Same protection on the transaction's own (switched) channel:
    // a Nagad SMS with the submitted TrxID but the wrong amount.
    let wrong_nagad = "You have received Tk 999.00 from 01712-345678. \
        Trx ID: 9HX7K2LMNP. Your new balance is Tk 5,430.50. - Nagad";
    let (status, body) = forward_sms(&app, &key, "Nagad", wrong_nagad).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "wrong-amount nagad SMS: {body}"
    );
    assert_eq!(body["outcome"], json!("unmatched"), "{body}");

    let view = get_checkout(&app, &tx).await;
    assert_eq!(
        view["status"],
        json!("processing"),
        "transaction must remain unsettled after wrong-amount SMS: {view}"
    );
}

// ---------------------------------------------------------------------------
// Coordinator fix round (B6 review F5): spec §7 pins B-T8
// (`gateway: null`), B-T11 (blank gateway string) and the B-T15
// first half (an invalid gateway on a non-Pending transaction is
// a 409 status conflict, never a 400 — status is checked first).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submit_gateway_null_and_blank_keep_creation_gateway() {
    let app = app();
    let (_t, _m, tx) = bkash_nagad_pending(&app, "edge-null@example.com").await;

    // Explicit JSON null.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/checkout/{tx}/submit"),
            None,
            None,
            Some(json!({
                "sender_number": CUSTOMER_PHONE,
                "provider_trx_id": SHARED_TRX,
                "gateway": null,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "null gateway submit: {body}");
    assert_eq!(body["transaction"]["gateway"], json!("bkash"), "{body}");
    assert_eq!(body["transaction"]["status"], json!("processing"), "{body}");

    // Blank string on a fresh transaction.
    let app = crate::app();
    let (_t, _m, tx) = bkash_nagad_pending(&app, "edge-blank@example.com").await;
    let (status, body) = submit(&app, &tx, SHARED_TRX, Some("   ")).await;
    assert_eq!(status, StatusCode::OK, "blank gateway submit: {body}");
    assert_eq!(body["transaction"]["gateway"], json!("bkash"), "{body}");
}

#[tokio::test]
async fn submit_invalid_gateway_on_processing_transaction_is_409_not_400() {
    let app = app();
    let (_t, _m, tx) = bkash_nagad_pending(&app, "edge-409@example.com").await;

    let (status, body) = submit(&app, &tx, SHARED_TRX, Some("bkash")).await;
    assert_eq!(status, StatusCode::OK, "first submit: {body}");

    // The transaction is now Processing: the status check runs
    // before gateway validation, so even a bogus gateway is a 409.
    let (status, body) = submit(&app, &tx, SHARED_TRX, Some("bogus-gateway")).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "second submit with invalid gateway must be 409: {body}"
    );
}
