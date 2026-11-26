//! Postgres persistence integration tests (Waves 72–73).
//!
//! These tests run against a REAL Postgres when `TEST_DATABASE_URL`
//! is set (CI provides one via a service container; locally, start
//! Postgres and export the variable). Without it they skip —
//! except in CI (`CI=true`), where a missing variable is a hard
//! failure so the suite can never silently go un-exercised.
//!
//! Coverage:
//! - snapshot → fresh state → hydrate round-trip, including the
//!   password hash (which API serialization deliberately omits);
//! - the full HTTP smoke flow (register → login → merchant) run
//!   against a Postgres-backed state, then a *restarted* router
//!   hydrated from the database: the same credentials still log
//!   in and the merchant is still there.

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use miraclepay_api::persist::{hydrate_from, snapshot_to};
use miraclepay_api::{build_router, AppState};

fn pg_url() -> Option<String> {
    match std::env::var("TEST_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => Some(url),
        _ => {
            if std::env::var("CI").is_ok() {
                panic!("TEST_DATABASE_URL must be set in CI (Postgres persistence tests)");
            }
            eprintln!("TEST_DATABASE_URL not set — skipping Postgres persistence tests");
            None
        }
    }
}

async fn fresh_pool(url: &str, suffix: &str) -> sqlx::PgPool {
    // One database per test so parallel tests never share mp_records.
    let (base, _db) = url.rsplit_once('/').expect("url has a db name");
    let admin = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .expect("connect to TEST_DATABASE_URL");
    let db_name = format!("mp_persist_{suffix}");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
        .execute(&admin)
        .await
        .expect("drop old test db");
    sqlx::query(&format!("CREATE DATABASE {db_name}"))
        .execute(&admin)
        .await
        .expect("create test db");
    admin.close().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect(&format!("{base}/{db_name}"))
        .await
        .expect("connect to test db");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    sqlx::query("DELETE FROM mp_records")
        .execute(&pool)
        .await
        .expect("clean mp_records");
    pool
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn snapshot_hydrate_round_trip_against_real_postgres() {
    let Some(url) = pg_url() else { return };
    let pool = fresh_pool(&url, "roundtrip").await;

    let source = AppState::new("pg-secret".to_string()).with_db_pool(Some(pool.clone()));
    {
        let mut users = source.users.write().await;
        users.insert(
            "usr_pg".to_string(),
            miraclepay_domain::user::User {
                id: "usr_pg".to_string(),
                name: "PG User".to_string(),
                email: "pg@example.com".to_string(),
                password_hash: miraclepay_domain::password::hash_password("hunter2pass"),
                role: miraclepay_domain::user::UserRole::Merchant,
                email_verified: true,
                suspended: false,
                totp_secret: None,
                totp_enabled: false,
                created_at: chrono::Utc::now(),
            },
        );
    }
    let written = snapshot_to(&pool, &source).await.expect("snapshot");
    assert!(written >= 2, "settings + user rows expected, got {written}");

    let target = AppState::new("pg-secret".to_string()).with_db_pool(Some(pool.clone()));
    let loaded = hydrate_from(&pool, &target).await.expect("hydrate");
    assert_eq!(loaded, written);
    let users = target.users.read().await;
    let user = users.get("usr_pg").expect("user restored");
    assert!(
        miraclepay_domain::password::verify_password("hunter2pass", &user.password_hash),
        "password hash must survive the round trip"
    );
}

#[tokio::test]
async fn http_flow_survives_restart_with_postgres() {
    let Some(url) = pg_url() else { return };
    let pool = fresh_pool(&url, "httpflow").await;

    // Boot 1: register + create a merchant over HTTP.
    let state1 = AppState::new("pg-secret".to_string()).with_db_pool(Some(pool.clone()));
    let app1 = build_router(state1.clone());
    let (status, body) = call(
        &app1,
        Method::POST,
        "/api/v1/auth/register",
        None,
        Some(
            json!({"name": "PG Merchant", "email": "owner@pg.example", "password": "password123"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register: {body}");
    let (status, body) = call(
        &app1,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({"email": "owner@pg.example", "password": "password123"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login: {body}");
    let token = body["token"].as_str().expect("jwt").to_string();
    let (status, body) = call(
        &app1,
        Method::POST,
        "/api/v1/merchants",
        Some(&token),
        Some(json!({"business_name": "PG Shop", "email": "shop@pg.example", "phone": "01700000000", "country": "BD"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "merchant: {body}");
    let merchant_id = body["id"].as_str().expect("merchant id").to_string();

    snapshot_to(&pool, &state1).await.expect("snapshot");

    // Boot 2 ("restart"): a brand-new state hydrated from Postgres.
    let state2 = AppState::new("pg-secret".to_string()).with_db_pool(Some(pool.clone()));
    let loaded = hydrate_from(&pool, &state2).await.expect("hydrate");
    assert!(
        loaded >= 3,
        "user + merchant + settings expected, got {loaded}"
    );
    let app2 = build_router(state2);
    let (status, body) = call(
        &app2,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({"email": "owner@pg.example", "password": "password123"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login after restart: {body}");
    let token2 = body["token"].as_str().expect("jwt").to_string();
    let (status, body) = call(
        &app2,
        Method::GET,
        &format!("/api/v1/merchants/{merchant_id}"),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "merchant after restart: {body}");
    assert_eq!(body["business_name"], json!("PG Shop"));
}

// Wave 136: records written by v0.0.1 (before fields like
// `suspended`, `evidence_files` and the payout schedule existed)
// must still hydrate in 0.0.2 — the serde defaults carry them.
#[tokio::test]
async fn v001_shaped_records_hydrate() {
    let Some(url) = pg_url() else {
        return;
    };
    let pool = fresh_pool(&url, "v001").await;
    // A v0.0.1 PersistUser record: no `suspended` field at all.
    let legacy_user = serde_json::json!({
        "id": "usr_legacy",
        "name": "Legacy User",
        "email": "legacy@example.com",
        "password_hash": "argon2id-legacy-hash",
        "role": "merchant",
        "email_verified": true,
        "totp_secret": null,
        "totp_enabled": false,
        "created_at": "2026-11-26T00:00:00Z"
    });
    sqlx::query("INSERT INTO mp_records (collection, id, data) VALUES ('users', 'usr_legacy', $1)")
        .bind(legacy_user)
        .execute(&pool)
        .await
        .expect("insert legacy record");

    let state = AppState::new("v001-secret".to_string()).with_db_pool(Some(pool.clone()));
    let loaded = hydrate_from(&pool, &state).await.expect("hydrate legacy");
    assert!(loaded >= 1);
    let users = state.users.read().await;
    let user = users.get("usr_legacy").expect("legacy user hydrated");
    assert!(!user.suspended, "missing field defaults to active");
    assert_eq!(user.email, "legacy@example.com");
}
