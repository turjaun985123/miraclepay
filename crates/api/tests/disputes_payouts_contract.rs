//! Contract tests for Disputes + Settlements (Payouts) — Stream C, seat C5.
//!
//! Pins the exact JSON contract the merchant dashboard UIs depend on:
//! field names, nesting, list-envelope shapes, and status strings.
//! Handler facts (verified against handlers/disputes.rs,
//! handlers/settlements.rs and the domain types):
//!
//! * Dispute object keys: `id, merchant_id, transaction_id, amount,
//!   reason, status, evidence_note, evidence_files, outcome_note,
//!   opened_at, decided_at`. There is NO `created_at` — the timestamp
//!   is `opened_at`. `amount` is a nested Money object
//!   `{amount_minor, currency}`, not a flat integer.
//! * Dispute statuses (snake_case): `opened`, `under_review`, `won`,
//!   `lost`, `closed`. Reasons: `fraudulent`, `product_not_received`,
//!   `duplicate`, `credit_not_processed`, `other`.
//! * `GET /disputes` returns an envelope `{items, total, page,
//!   per_page, total_pages}` (the last three are C2's additive page
//!   metadata). List rows and the detail are `DisputeView`s: the 11
//!   base fields plus `merchant_name`, `transaction_reference`,
//!   `provider_trx_id`, `gateway`, `customer_email`, `customer_phone`.
//!   `POST /disputes` and the evidence POSTs still return the plain
//!   11-field `Dispute`.
//! * Text evidence: `POST /disputes/:id/evidence` with `{note}` sets
//!   the single `evidence_note` field (it does NOT append to a list)
//!   and moves `opened` -> `under_review`.
//! * File evidence: `POST /disputes/:id/evidence/files` with JSON
//!   `{name, url}` (https URL reference — NOT multipart/bytes) appends
//!   to `evidence_files[]` (`{name, url, added_at}`).
//! * Settlement object keys: `id, merchant_id, gross, fee, status,
//!   period_start, period_end, payout_method, payout_reference,
//!   created_at`. `gross`/`fee` are nested Money objects; there is NO
//!   flat `amount` field (the derived net IS serialized on the
//!   list view as `net`/`net_minor`; see below).
//! * Settlement statuses (snake_case): `pending`, `processing`,
//!   `paid`, `failed`, `on_hold`. Settlements built by a run start
//!   `pending` with `payout_method: "manual"`.
//! * `GET /settlements` returns a BARE JSON array (no envelope) and
//!   IS merchant-scoped (owner or admin, same `may_access` rule as
//!   disputes). List items are `SettlementView`s: the 10 base fields
//!   plus `net` (Money), `net_minor` (i64) and `merchant_name` —
//!   C2's enrichment. `POST /settlements/run` still embeds the plain
//!   10-field settlements, so run and list shapes differ by exactly
//!   those three keys.
//! * Cross-merchant dispute read returns 404 (not 403): the handler
//!   deliberately hides existence via `load_visible`.

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use miraclepay_api::{build_router, AppState};

const TEST_SECRET: &str = "disputes-payouts-contract-secret";
const MERCHANT_BKASH_NUMBER: &str = "01700000000";
const CUSTOMER_PHONE: &str = "01712345678";
const AMOUNT_MINOR: i64 = 125_000;
/// SMS body for a given provider TrxID. Each completed-transaction
/// setup in one app MUST use a distinct TrxID: re-sending an identical
/// SMS is (correctly) answered `{"outcome":"duplicate"}` and does not
/// complete the second transaction.
fn bkash_sms_body(trx_id: &str) -> String {
    format!("You have received Tk 1,250.00 from 01712345678. TrxID: {trx_id}. Balance: Tk 5,430.50. Thank you for using bKash.")
}
const PERIOD_START: &str = "2020-01-01T00:00:00Z";
const PERIOD_END: &str = "2030-01-01T00:00:00Z";

// ---------------------------------------------------------------------------
// Harness (mirrors integration.rs)
// ---------------------------------------------------------------------------

fn app() -> Router {
    build_router(AppState::new(TEST_SECRET.to_string()))
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
    let response = app.clone().oneshot(req).await.expect("router must answer");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body readable");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("body must be JSON")
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
                "name": "Contract Merchant", "email": email, "password": "password123",
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
            Some(json!({
                "email": email, "password": "password123",
            })),
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
                "business_name": "Dhaka Contract Shop",
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
                "gateway": "bkash", "merchant_number": MERCHANT_BKASH_NUMBER,
                "api_key": null, "api_secret": null, "extra": {}, "sandbox": true, "enabled": true,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "configure bkash: {body}");
    assert_eq!(body["configured"], json!(true));
}

/// Proven path to a COMPLETED transaction (mirrors integration.rs
/// `public_payment_link_flow_settles_via_sms`): payment link -> public
/// pay (pending) -> checkout submit -> device SMS (verified).
/// Returns (token, merchant_id, transaction_id).
async fn setup_completed_transaction(
    app: &Router,
    email: &str,
    trx_id: &str,
) -> (String, String, String) {
    let token = register_and_login(app, email).await;
    let merchant_id = create_merchant(app, &token).await;
    configure_bkash(app, &token, &merchant_id).await;

    let (status, body) = send(app, request(Method::POST, "/api/v1/payment-links", Some(&token), None, Some(json!({
        "merchant_id": merchant_id, "title": "Contract Widget", "description": "one",
        "amount_minor": AMOUNT_MINOR, "currency": "BDT", "collect_phone": true, "collect_email": false,
    })))).await;
    assert_eq!(status, StatusCode::CREATED, "create link: {body}");
    let slug = body["slug"].as_str().expect("slug").to_string();

    let (status, body) = send(
        app,
        request(
            Method::POST,
            &format!("/api/v1/payment-links/public/{slug}/pay"),
            None,
            None,
            Some(json!({
                "gateway": "bkash", "customer_phone": CUSTOMER_PHONE,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "public pay: {body}");
    assert_eq!(body["status"], json!("pending"));
    let tx_id = body["transaction_id"].as_str().expect("tx id").to_string();

    let (status, body) = send(
        app,
        request(
            Method::POST,
            &format!("/api/v1/checkout/{tx_id}/submit"),
            None,
            None,
            Some(json!({
                "sender_number": CUSTOMER_PHONE, "provider_trx_id": trx_id,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "checkout submit: {body}");

    let (status, body) = send(app, request(Method::POST, "/api/v1/api-keys", Some(&token), None, Some(json!({
        "merchant_id": merchant_id, "name": "Android SMS forwarder", "scopes": ["payments_write"],
    })))).await;
    assert_eq!(status, StatusCode::CREATED, "create api key: {body}");
    let device_key = body["key"].as_str().expect("device key").to_string();

    let (status, body) = send(app, request(Method::POST, "/api/v1/sms", None, Some(&device_key), Some(json!({
        "device_id": "contract-android-device", "sender": "bKash", "body": bkash_sms_body(trx_id),
        "received_at": "2026-11-26T03:16:00Z", "sim_slot": null,
    })))).await;
    assert_eq!(status, StatusCode::OK, "sms intake: {body}");
    assert_eq!(
        body["outcome"],
        json!("verified"),
        "sms must verify: {body}"
    );

    let (status, body) = send(
        app,
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
    assert_eq!(
        body["transaction"]["status"],
        json!("completed"),
        "tx must be completed: {body}"
    );
    (token, merchant_id, tx_id)
}

async fn open_dispute(app: &Router, token: &str, tx_id: &str) -> (StatusCode, Value) {
    send(app, request(Method::POST, "/api/v1/disputes", Some(token), None, Some(json!({
        "transaction_id": tx_id, "amount_minor": AMOUNT_MINOR, "currency": "BDT", "reason": "fraudulent",
    })))).await
}

async fn run_settlements(app: &Router, token: &str, merchant_id: &str) -> (StatusCode, Value) {
    send(
        app,
        request(
            Method::POST,
            "/api/v1/settlements/run",
            Some(token),
            None,
            Some(json!({
                "merchant_id": merchant_id, "period_start": PERIOD_START, "period_end": PERIOD_END,
            })),
        ),
    )
    .await
}

const DISPUTE_BASE_KEYS: &[&str] = &[
    "amount",
    "decided_at",
    "evidence_files",
    "evidence_note",
    "id",
    "merchant_id",
    "opened_at",
    "outcome_note",
    "reason",
    "status",
    "transaction_id",
];
const DISPUTE_VIEW_KEYS: &[&str] = &[
    "amount",
    "customer_email",
    "customer_phone",
    "decided_at",
    "evidence_files",
    "evidence_note",
    "gateway",
    "id",
    "merchant_id",
    "merchant_name",
    "opened_at",
    "outcome_note",
    "provider_trx_id",
    "reason",
    "status",
    "transaction_id",
    "transaction_reference",
];

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v
        .as_object()
        .expect("must be an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn assert_keys(v: &Value, expected: &[&str]) {
    let mut exp: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    exp.sort();
    assert_eq!(sorted_keys(v), exp, "exact key set mismatch for: {v}");
}

// ---------------------------------------------------------------------------
// 1. Merchant scoping — disputes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn disputes_list_is_merchant_scoped() {
    let app = app();
    let (token_a, _m_a, tx_a) =
        setup_completed_transaction(&app, "dispute-a@example.com", "9HX7K2LMNA").await;
    let (token_b, _m_b, tx_b) =
        setup_completed_transaction(&app, "dispute-b@example.com", "9HX7K2LMNB").await;

    let (status, dispute_a) = open_dispute(&app, &token_a, &tx_a).await;
    assert_eq!(status, StatusCode::CREATED, "open A: {dispute_a}");
    let (status, dispute_b) = open_dispute(&app, &token_b, &tx_b).await;
    assert_eq!(status, StatusCode::CREATED, "open B: {dispute_b}");
    let id_a = dispute_a["id"].as_str().unwrap().to_string();
    let id_b = dispute_b["id"].as_str().unwrap().to_string();
    assert_ne!(id_a, id_b);

    for (token, own_id, other_id, own_tx) in [
        (&token_a, &id_a, &id_b, &tx_a),
        (&token_b, &id_b, &id_a, &tx_b),
    ] {
        let (status, body) = send(
            &app,
            request(Method::GET, "/api/v1/disputes", Some(token), None, None),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "list: {body}");
        assert_keys(
            &body,
            &["items", "page", "per_page", "total", "total_pages"],
        );
        assert_eq!(
            body["total"],
            json!(1),
            "each merchant sees exactly own dispute: {body}"
        );
        let items = body["items"].as_array().expect("items array");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], json!(own_id));
        assert_eq!(items[0]["transaction_id"], json!(own_tx));
        assert!(
            items.iter().all(|d| d["id"] != json!(other_id)),
            "other merchant's dispute leaked: {body}"
        );
    }

    // Opening a dispute against another merchant's transaction is
    // hidden as 404 (same existence-hiding as the read path).
    let (status, body) = open_dispute(&app, &token_b, &tx_a).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-merchant open must be 404: {body}"
    );
}

// ---------------------------------------------------------------------------
// 1b. Merchant scoping — settlements (merchant-scoped by design)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn settlements_list_is_merchant_scoped() {
    let app = app();
    let (token_a, merchant_a, _tx_a) =
        setup_completed_transaction(&app, "settle-a@example.com", "9HX7K2LMNC").await;
    let (token_b, merchant_b, _tx_b) =
        setup_completed_transaction(&app, "settle-b@example.com", "9HX7K2LMND").await;

    let (status, run_a) = run_settlements(&app, &token_a, &merchant_a).await;
    assert_eq!(status, StatusCode::CREATED, "run A: {run_a}");
    assert_eq!(run_a["created"], json!(1), "run A: {run_a}");
    let settlement_a = run_a["settlements"][0]["id"].as_str().unwrap().to_string();
    let (status, run_b) = run_settlements(&app, &token_b, &merchant_b).await;
    assert_eq!(status, StatusCode::CREATED, "run B: {run_b}");
    let settlement_b = run_b["settlements"][0]["id"].as_str().unwrap().to_string();

    for (token, own_id, other_id, own_merchant) in [
        (&token_a, &settlement_a, &settlement_b, &merchant_a),
        (&token_b, &settlement_b, &settlement_a, &merchant_b),
    ] {
        let (status, body) = send(
            &app,
            request(Method::GET, "/api/v1/settlements", Some(token), None, None),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "list settlements: {body}");
        let items = body
            .as_array()
            .expect("settlements list must be a bare JSON array (no envelope)");
        assert_eq!(
            items.len(),
            1,
            "each merchant sees exactly own settlement: {body}"
        );
        assert_eq!(items[0]["id"], json!(own_id));
        assert_eq!(items[0]["merchant_id"], json!(own_merchant));
        assert!(
            items.iter().all(|s| s["id"] != json!(other_id)),
            "other merchant's settlement leaked: {body}"
        );
        assert!(items
            .iter()
            .all(|s| s["merchant_id"] == json!(own_merchant)));
    }
}

// ---------------------------------------------------------------------------
// 2. Dispute lifecycle + table/detail fields + evidence + status strings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispute_lifecycle_fields_evidence_and_status_strings() {
    let app = app();
    let (token, merchant_id, tx_id) =
        setup_completed_transaction(&app, "lifecycle@example.com", "9HX7K2LMNE").await;
    let trx_id = "9HX7K2LMNE";

    // Open on a completed transaction.
    let (status, dispute) = open_dispute(&app, &token, &tx_id).await;
    assert_eq!(status, StatusCode::CREATED, "open dispute: {dispute}");
    // POST returns the plain Dispute: exact base field inventory.
    assert_keys(&dispute, DISPUTE_BASE_KEYS);
    let dispute_id = dispute["id"].as_str().expect("id").to_string();
    assert!(
        dispute_id.starts_with("dsp"),
        "dispute id prefix: {dispute_id}"
    );
    assert_eq!(dispute["transaction_id"], json!(tx_id));
    assert_eq!(dispute["merchant_id"], json!(merchant_id));
    // Amount is nested Money — UI must read amount.amount_minor.
    assert_keys(&dispute["amount"], &["amount_minor", "currency"]);
    assert_eq!(dispute["amount"]["amount_minor"], json!(AMOUNT_MINOR));
    assert_eq!(dispute["amount"]["currency"], json!("BDT"));
    assert_eq!(dispute["reason"], json!("fraudulent"));
    // Exact status string at creation.
    assert_eq!(dispute["status"], json!("opened"));
    assert!(
        dispute["opened_at"].as_str().is_some_and(|s| !s.is_empty()),
        "opened_at (not created_at) must be present: {dispute}"
    );
    assert_eq!(dispute["decided_at"], Value::Null);
    assert_eq!(dispute["evidence_note"], Value::Null);
    assert_eq!(dispute["evidence_files"], json!([]));
    assert_eq!(dispute["outcome_note"], Value::Null);

    // Appears in the list with the same table fields.
    let (status, list) = send(
        &app,
        request(Method::GET, "/api/v1/disputes", Some(&token), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], json!(1));
    assert_keys(
        &list,
        &["items", "page", "per_page", "total", "total_pages"],
    );
    let row = &list["items"][0];
    assert_keys(row, DISPUTE_VIEW_KEYS);
    assert_eq!(row["merchant_name"], json!("Dhaka Contract Shop"));
    assert_eq!(row["gateway"], json!("bkash"));
    assert_eq!(row["id"], json!(dispute_id));
    assert_eq!(row["transaction_id"], json!(tx_id));
    assert_eq!(row["amount"]["amount_minor"], json!(AMOUNT_MINOR));
    assert_eq!(row["status"], json!("opened"));
    assert!(row["opened_at"].as_str().is_some());

    // GET detail includes the evidence fields.
    let (status, detail) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/disputes/{dispute_id}"),
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "detail: {detail}");
    assert_keys(&detail, DISPUTE_VIEW_KEYS);
    assert_eq!(detail["evidence_files"], json!([]));
    // Joined display fields (C2 DisputeView).
    assert_eq!(detail["merchant_name"], json!("Dhaka Contract Shop"));
    assert_eq!(detail["gateway"], json!("bkash"));
    assert_eq!(detail["provider_trx_id"], json!(trx_id));
    assert_eq!(detail["customer_phone"], json!(CUSTOMER_PHONE));

    // POST text evidence: sets evidence_note, opened -> under_review.
    let (status, after_note) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/disputes/{dispute_id}/evidence"),
            Some(&token),
            None,
            Some(json!({
                "note": "Customer confirmed delivery on 2026-11-26; see tracking.",
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add evidence: {after_note}");
    assert_eq!(
        after_note["evidence_note"],
        json!("Customer confirmed delivery on 2026-11-26; see tracking.")
    );
    assert_eq!(
        after_note["status"],
        json!("under_review"),
        "exact status after evidence: {after_note}"
    );
    assert_eq!(after_note["evidence_files"], json!([]));

    // POST file evidence: JSON {name, url} reference (not multipart),
    // appends to evidence_files.
    let (status, after_file) = send(&app, request(Method::POST, &format!("/api/v1/disputes/{dispute_id}/evidence/files"), Some(&token), None, Some(json!({
        "name": "delivery-receipt.pdf", "url": "https://example.com/evidence/delivery-receipt.pdf",
    })))).await;
    assert_eq!(status, StatusCode::OK, "add evidence file: {after_file}");
    assert_eq!(after_file["status"], json!("under_review"));
    let files = after_file["evidence_files"]
        .as_array()
        .expect("evidence_files array");
    assert_eq!(files.len(), 1, "file evidence must append: {after_file}");
    assert_keys(&files[0], &["added_at", "name", "url"]);
    assert_eq!(files[0]["name"], json!("delivery-receipt.pdf"));
    assert_eq!(
        files[0]["url"],
        json!("https://example.com/evidence/delivery-receipt.pdf")
    );
    assert!(files[0]["added_at"].as_str().is_some());

    // A second file appends (list grows to 2); detail reflects both
    // evidence kinds.
    let (status, _) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/disputes/{dispute_id}/evidence/files"),
            Some(&token),
            None,
            Some(json!({
                "name": "tracking.png", "url": "https://example.com/evidence/tracking.png",
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, detail) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/disputes/{dispute_id}"),
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["evidence_files"].as_array().unwrap().len(), 2);
    assert_eq!(
        detail["evidence_note"],
        json!("Customer confirmed delivery on 2026-11-26; see tracking.")
    );
    assert_eq!(detail["status"], json!("under_review"));
}

// ---------------------------------------------------------------------------
// 3. Settlements list contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn settlements_list_returns_payouts_table_fields() {
    let app = app();
    let (token, merchant_id, _tx_id) =
        setup_completed_transaction(&app, "payouts-table@example.com", "9HX7K2LMNF").await;

    let (status, run) = run_settlements(&app, &token, &merchant_id).await;
    assert_eq!(status, StatusCode::CREATED, "run: {run}");
    assert_keys(&run, &["created", "settlements"]);
    assert_eq!(run["created"], json!(1));

    let (status, body) = send(
        &app,
        request(Method::GET, "/api/v1/settlements", Some(&token), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list: {body}");
    // Bare array, NOT the {items,total} envelope disputes use.
    let items = body
        .as_array()
        .expect("settlements list must be a bare array");
    assert_eq!(items.len(), 1);
    let s = &items[0];
    // List items are SettlementViews: base fields + net/net_minor/merchant_name.
    assert_keys(
        s,
        &[
            "created_at",
            "fee",
            "gross",
            "id",
            "merchant_id",
            "merchant_name",
            "net",
            "net_minor",
            "period_end",
            "period_start",
            "payout_method",
            "payout_reference",
            "status",
        ],
    );
    assert_eq!(s["merchant_name"], json!("Dhaka Contract Shop"));
    // Net is derived gross - fee, exposed both as Money and flat minor.
    let gross_minor = s["gross"]["amount_minor"].as_i64().unwrap();
    let fee_minor = s["fee"]["amount_minor"].as_i64().unwrap();
    assert_keys(&s["net"], &["amount_minor", "currency"]);
    assert_eq!(s["net"]["amount_minor"], json!(gross_minor - fee_minor));
    assert_eq!(s["net"]["currency"], json!("BDT"));
    assert_eq!(s["net_minor"], json!(gross_minor - fee_minor));
    assert!(s["id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(s["merchant_id"], json!(merchant_id));
    // Amounts are nested Money under gross/fee — no flat `amount`, no `net`.
    assert_keys(&s["gross"], &["amount_minor", "currency"]);
    assert_eq!(s["gross"]["amount_minor"], json!(AMOUNT_MINOR));
    assert_eq!(s["gross"]["currency"], json!("BDT"));
    assert_keys(&s["fee"], &["amount_minor", "currency"]);
    assert_eq!(s["fee"]["currency"], json!("BDT"));
    assert!(s["fee"]["amount_minor"]
        .as_i64()
        .is_some_and(|f| (0..=AMOUNT_MINOR).contains(&f)));
    // Exact status string for a freshly built settlement.
    assert_eq!(s["status"], json!("pending"));
    assert_eq!(s["payout_method"], json!("manual"));
    assert_eq!(s["payout_reference"], Value::Null);
    // Schedule / timestamp fields the payouts table needs.
    assert_eq!(s["period_start"], json!(PERIOD_START));
    assert_eq!(s["period_end"], json!(PERIOD_END));
    assert!(s["created_at"].as_str().is_some_and(|v| !v.is_empty()));
    // The run response embeds the PLAIN settlement (10 base keys, no
    // view enrichment); its core fields match the list view's.
    assert_keys(
        &run["settlements"][0],
        &[
            "created_at",
            "fee",
            "gross",
            "id",
            "merchant_id",
            "period_end",
            "period_start",
            "payout_method",
            "payout_reference",
            "status",
        ],
    );
    for key in [
        "id",
        "merchant_id",
        "gross",
        "fee",
        "status",
        "period_start",
        "period_end",
        "payout_method",
        "payout_reference",
    ] {
        assert_eq!(
            run["settlements"][0][key], s[key],
            "run vs list mismatch on {key}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Auth: unauthenticated 401 + cross-merchant read hidden
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unauthenticated_disputes_and_settlements_lists_are_401() {
    let app = app();
    for uri in ["/api/v1/disputes", "/api/v1/settlements"] {
        let (status, body) = send(&app, request(Method::GET, uri, None, None, None)).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "GET {uri} without auth: {body}"
        );
        assert_eq!(body["error"], json!("unauthorized"), "GET {uri}: {body}");
    }
}

#[tokio::test]
async fn merchant_cannot_read_other_merchants_dispute_by_id() {
    let app = app();
    let (token_a, _m_a, tx_a) =
        setup_completed_transaction(&app, "reader-a@example.com", "9HX7K2LMNG").await;
    let (token_b, _m_b, _tx_b) =
        setup_completed_transaction(&app, "reader-b@example.com", "9HX7K2LMNH").await;
    let (status, dispute) = open_dispute(&app, &token_a, &tx_a).await;
    assert_eq!(status, StatusCode::CREATED);
    let dispute_id = dispute["id"].as_str().unwrap().to_string();

    // Actual API behaviour: 404 (existence-hiding), NOT 403.
    let (status, body) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/disputes/{dispute_id}"),
            Some(&token_b),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-merchant dispute read must be 404: {body}"
    );

    // Cross-merchant evidence submission is likewise hidden as 404.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/disputes/{dispute_id}/evidence"),
            Some(&token_b),
            None,
            Some(json!({
                "note": "should not land",
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-merchant evidence must be 404: {body}"
    );

    // Owner still reads it fine.
    let (status, body) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/disputes/{dispute_id}"),
            Some(&token_a),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "owner read: {body}");
    assert_eq!(
        body["evidence_note"],
        Value::Null,
        "blocked evidence must not land: {body}"
    );
}

// ---------------------------------------------------------------------------
// 7. Settlement write-endpoint hardening (Stream C S4/S5, v0.0.4):
//    a merchant must not waive their own platform fee, nor mark
//    their own settlement paid.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn merchant_cannot_set_fee_bps_or_mark_own_settlement_paid() {
    let app = app();
    let (token, merchant_id, _tx_id) =
        setup_completed_transaction(&app, "contract-hardening@example.com", "HARDEN1TRX").await;

    // S4: a non-admin supplying fee_bps is rejected outright.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/api/v1/settlements/run",
            Some(&token),
            None,
            Some(json!({
                "merchant_id": merchant_id, "period_start": PERIOD_START,
                "period_end": PERIOD_END, "fee_bps": 0,
            })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "merchant-supplied fee_bps must be rejected: {body}"
    );

    // A clean run (fee resolved from platform settings) still works.
    let (status, run) = run_settlements(&app, &token, &merchant_id).await;
    assert_eq!(status, StatusCode::CREATED, "clean run: {run}");
    let settlement_id = run["settlements"][0]["id"]
        .as_str()
        .expect("settlement id")
        .to_string();

    // S5: the merchant owner cannot mark their own settlement paid.
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/settlements/{settlement_id}/pay"),
            Some(&token),
            None,
            Some(json!({ "payout_reference": "SELF-PAY-1" })),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "merchant self-pay must be rejected: {body}"
    );

    // The settlement is untouched: still pending, no reference.
    let (status, body) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/settlements/{settlement_id}"),
            Some(&token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get settlement: {body}");
    assert_eq!(body["status"], json!("pending"), "{body}");
    assert_eq!(body["payout_reference"], Value::Null, "{body}");
}

// ---------------------------------------------------------------------------
// Coordinator fix round (C6 review F2 + F7): fee-override resolution,
// admin positive controls for the S4/S5 gates, settlement GET
// existence-hiding, the S3 snake_case status filter, and the
// disputes server-side filters.
// ---------------------------------------------------------------------------

/// Build a router whose state has a seeded platform admin
/// (`admin@example.com` / `adminpass123`) and return it with the
/// admin's bearer token.
async fn app_with_admin() -> (Router, String) {
    let state = AppState::new(TEST_SECRET.to_string());
    state
        .seed_admin(
            "admin@example.com",
            miraclepay_domain::password::hash_password("adminpass123"),
        )
        .await;
    let app = build_router(state);
    let (status, body) = send(
        &app,
        request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({ "email": "admin@example.com", "password": "adminpass123" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin login: {body}");
    let token = body["token"].as_str().expect("admin token").to_string();
    (app, token)
}

/// F7(a) — admin positive controls: the S4/S5 gates must not break
/// the admin's own powers. An admin CAN set fee_bps on a run and
/// CAN mark a settlement paid.
#[tokio::test]
async fn admin_can_set_fee_bps_and_pay_settlement() {
    let (app, admin_token) = app_with_admin().await;
    let (token, merchant_id, _tx_id) =
        setup_completed_transaction(&app, "admin-controls@example.com", "ADMINCTL1").await;

    let (status, run) = send(
        &app,
        request(
            Method::POST,
            "/api/v1/settlements/run",
            Some(&admin_token),
            None,
            Some(json!({
                "merchant_id": merchant_id, "period_start": PERIOD_START,
                "period_end": PERIOD_END, "fee_bps": 100,
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "admin run with fee_bps: {run}");
    let settlement = &run["settlements"][0];
    // 1% of 125_000 = 1_250 exactly.
    assert_eq!(settlement["fee"]["amount_minor"], json!(1_250), "{run}");
    let settlement_id = settlement["id"]
        .as_str()
        .expect("settlement id")
        .to_string();

    let (status, body) = send(
        &app,
        request(
            Method::POST,
            &format!("/api/v1/settlements/{settlement_id}/pay"),
            Some(&admin_token),
            None,
            Some(json!({ "payout_reference": "ADMIN-PAY-1" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin pay: {body}");
    assert_eq!(body["status"], json!("paid"), "{body}");
    assert_eq!(body["payout_reference"], json!("ADMIN-PAY-1"), "{body}");
    let _ = token;
}

/// F2 — a merchant-initiated run with no `fee_bps` charges the
/// merchant's own settings override, not the platform default.
#[tokio::test]
async fn run_uses_merchant_fee_bps_override() {
    let app = app();
    let (token, merchant_id, _tx_id) =
        setup_completed_transaction(&app, "fee-override@example.com", "FEEOVR123").await;

    // Set a 150 bps override through the settings API
    // (GET → mutate → PUT, the full-settings round-trip).
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
    settings["fee_bps"] = json!(150);
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

    let (status, run) = run_settlements(&app, &token, &merchant_id).await;
    assert_eq!(status, StatusCode::CREATED, "run: {run}");
    // 150 bps of 125_000 = 1_875 (the platform default 250 bps
    // would charge 3_125).
    assert_eq!(
        run["settlements"][0]["fee"]["amount_minor"],
        json!(1_875),
        "run must charge the merchant override: {run}"
    );
}

/// F7(b)+(c) — `GET /settlements/:id` hides other merchants'
/// settlements behind 404, and the list status filter accepts the
/// snake_case spellings the stored statuses actually use (S3).
#[tokio::test]
async fn settlement_get_hides_cross_merchant_and_status_filter_is_snake_case() {
    let app = app();
    let (token_a, merchant_a, _tx) =
        setup_completed_transaction(&app, "settle-scope-a@example.com", "SETSCOPEA").await;
    let (token_b, _merchant_b, _tx_b) =
        setup_completed_transaction(&app, "settle-scope-b@example.com", "SETSCOPEB").await;

    let (status, run) = run_settlements(&app, &token_a, &merchant_a).await;
    assert_eq!(status, StatusCode::CREATED, "run: {run}");
    let settlement_id = run["settlements"][0]["id"]
        .as_str()
        .expect("settlement id")
        .to_string();

    // Other merchant: 404, not 403 — existence is hidden.
    let (status, _body) = send(
        &app,
        request(
            Method::GET,
            &format!("/api/v1/settlements/{settlement_id}"),
            Some(&token_b),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "cross-merchant get must 404");

    // S3: snake_case filter values match; the on_hold spelling is
    // accepted (empty here — no settlement is on hold).
    let (status, body) = send(
        &app,
        request(
            Method::GET,
            "/api/v1/settlements?status=pending",
            Some(&token_a),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "pending filter: {body}");
    assert_eq!(body.as_array().expect("array").len(), 1, "{body}");
    let (status, body) = send(
        &app,
        request(
            Method::GET,
            "/api/v1/settlements?status=on_hold",
            Some(&token_a),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "on_hold filter: {body}");
    assert_eq!(body.as_array().expect("array").len(), 0, "{body}");
}

/// F7(d) — the disputes list honours the server-side merchant_id /
/// status / reason / transaction_id filters within the caller's
/// scope.
#[tokio::test]
async fn disputes_list_filters_are_applied() {
    let app = app();
    let (token, merchant_id, tx_id) =
        setup_completed_transaction(&app, "dispute-filters@example.com", "DISPFILT1").await;
    let (status, dispute) = open_dispute(&app, &token, &tx_id).await;
    assert_eq!(status, StatusCode::CREATED, "open dispute: {dispute}");
    let dispute_id = dispute["id"].as_str().expect("dispute id").to_string();

    async fn list(app: &Router, token: &str, qs: &str) -> Value {
        let (status, body) = send(
            app,
            request(
                Method::GET,
                &format!("/api/v1/disputes?{qs}"),
                Some(token),
                None,
                None,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "list {qs}: {body}");
        body
    }

    let body = list(&app, &token, "status=opened").await;
    assert_eq!(body["total"], json!(1), "{body}");
    assert_eq!(body["items"][0]["id"], json!(dispute_id), "{body}");
    let body = list(&app, &token, "status=won").await;
    assert_eq!(body["total"], json!(0), "{body}");
    let body = list(&app, &token, "reason=fraudulent").await;
    assert_eq!(body["total"], json!(1), "{body}");
    let body = list(&app, &token, "reason=duplicate").await;
    assert_eq!(body["total"], json!(0), "{body}");
    let body = list(&app, &token, &format!("transaction_id={tx_id}")).await;
    assert_eq!(body["total"], json!(1), "{body}");
    let body = list(&app, &token, "transaction_id=txn_nope").await;
    assert_eq!(body["total"], json!(0), "{body}");
    let body = list(&app, &token, &format!("merchant_id={merchant_id}")).await;
    assert_eq!(body["total"], json!(1), "{body}");
    let body = list(&app, &token, "merchant_id=mrc_nope").await;
    assert_eq!(body["total"], json!(0), "{body}");
}
