//! Integration tests for the embedded frontend (v0.0.3.2).
//!
//! The released binary serves the Astro UI from the same process
//! as the API. These tests drive the real router in-process and
//! pin the contract:
//!
//! - `GET /` returns the landing page HTML (200, text/html).
//! - A real page route (`/login/`) returns its HTML — asserted
//!   whenever the real bundle is embedded; on a machine where the
//!   frontend was never built, `build.rs` embeds a placeholder
//!   index only, so page assertions relax (the resolution logic
//!   itself is fixture-tested in `static_files` unit tests, and CI
//!   builds the real bundle before compiling, then curls the
//!   release binary in `scripts/embedded-ui-smoke.sh`).
//! - `GET /health` stays JSON; unknown `/api/v1/*` stays a JSON
//!   404 — API behaviour is untouched by the static fallback.
//! - CSP is scoped: API responses keep `default-src 'none'`,
//!   frontend responses get the same-origin policy.
//! - A hashed `_astro/` asset serves with its real content type
//!   and the immutable cache header.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use miraclepay_api::static_files;
use miraclepay_api::{build_router, AppState};

const TEST_SECRET: &str = "static-ui-test-secret";

fn app() -> Router {
    build_router(AppState::new(TEST_SECRET.to_string()))
}

async fn get(path: &str) -> (StatusCode, Option<String>, Option<String>, String) {
    let response = app()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let csp = response
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        content_type,
        csp,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

#[tokio::test]
async fn root_serves_the_landing_html() {
    let (status, content_type, csp, body) = get("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type
            .as_deref()
            .unwrap_or("")
            .starts_with("text/html"),
        "content-type: {content_type:?}"
    );
    assert!(
        body.contains("MiraclePay"),
        "landing body should name the product"
    );
    // The strict API CSP would break the page; the frontend policy
    // must be the same-origin one instead.
    let csp = csp.expect("frontend response carries a CSP");
    assert!(csp.contains("default-src 'self'"), "frontend CSP: {csp}");
    assert!(!csp.contains("default-src 'none'"), "frontend CSP: {csp}");
}

#[tokio::test]
async fn a_real_page_route_serves_its_html() {
    if static_files::is_placeholder_bundle() {
        // No frontend build on this machine: only the placeholder
        // index exists. CI covers the real pages via the release
        // binary smoke script.
        return;
    }
    assert!(static_files::has_embedded("login/index.html"));
    let (status, content_type, _, body) = get("/login/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type
        .as_deref()
        .unwrap_or("")
        .starts_with("text/html"));
    assert!(body.contains("MiraclePay"));
}

#[tokio::test]
async fn health_stays_json_with_the_strict_api_csp() {
    let (status, content_type, csp, body) = get("/health").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type
        .as_deref()
        .unwrap_or("")
        .starts_with("application/json"));
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("health is JSON");
    assert_eq!(parsed["status"], "ok");
    let csp = csp.expect("API response carries a CSP");
    assert!(csp.contains("default-src 'none'"), "API CSP: {csp}");
}

#[tokio::test]
async fn unknown_api_path_stays_a_json_404() {
    let (status, content_type, _, body) = get("/api/v1/definitely-not-a-route").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(content_type
        .as_deref()
        .unwrap_or("")
        .starts_with("application/json"));
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("API 404 is JSON");
    assert_eq!(parsed["error"], "not_found");
}

#[tokio::test]
async fn hashed_astro_asset_serves_with_type_and_immutable_cache() {
    let asset = static_files::embedded_asset_names()
        .into_iter()
        .find(|name| name.starts_with("_astro/") && name.ends_with(".js"));
    let Some(asset) = asset else {
        // Placeholder bundle: no hashed assets exist locally.
        return;
    };
    let response = app()
        .oneshot(
            Request::get(format!("/{asset}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("text/javascript"),
        "asset content-type: {content_type}"
    );
    let cache = response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(cache.contains("immutable"), "asset cache-control: {cache}");
}

#[tokio::test]
async fn unknown_browser_path_is_an_html_404() {
    let (status, content_type, _, body) = get("/no-such-page/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(content_type
        .as_deref()
        .unwrap_or("")
        .starts_with("text/html"));
    assert!(body.contains("MiraclePay"));
}
