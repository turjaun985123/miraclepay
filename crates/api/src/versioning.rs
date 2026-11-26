//! API versioning (Wave 40).
//!
//! MiraclePay's API is versioned in the path: everything lives
//! under `/api/v1`. This module formalises the contract:
//!
//! - `GET /api/versions` (public) reports the current and
//!   supported versions so clients can discover them instead of
//!   guessing.
//! - Every `/api/v1` response carries an `API-Version: v1`
//!   header (added by [`version_middleware`]).
//! - Requests to an unsupported version (`/api/v2/...`, …) get a
//!   structured 404 naming the supported versions, instead of a
//!   bare "not found" that looks like a typo'd path.
//!
//! The policy (additive-only within a version, new version for
//! breaking changes, overlap window) lives in
//! `docs/api-versioning.md`.

use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;

/// The current (and only) API version.
pub const CURRENT: &str = "v1";
/// All versions this deployment serves.
pub const SUPPORTED: &[&str] = &["v1"];
/// Response header announcing the serving version.
pub const VERSION_HEADER: &str = "api-version";

/// `GET /api/versions` — version discovery.
pub async fn versions() -> Json<serde_json::Value> {
    Json(json!({
        "current": CURRENT,
        "supported": SUPPORTED,
        "deprecated": [],
        "policy": "Additive changes ship within v1; breaking changes ship as a new version with an overlap window. See docs/api-versioning.md.",
    }))
}

/// What the fallback should say for an unmatched path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackKind {
    /// A versioned path for a version we do not serve.
    UnsupportedVersion,
    /// Anything else unmatched.
    NotFound,
}

/// Classify an unmatched path: `/api/v2/...` (any `v<digits>`
/// other than the current version) is an unsupported-version
/// request; everything else is a plain 404.
pub fn classify_path(path: &str) -> FallbackKind {
    if let Some(rest) = path.strip_prefix("/api/") {
        if let Some(version) = rest.split('/').next() {
            let is_version_shape = version.len() >= 2
                && version.starts_with('v')
                && version[1..].chars().all(|c| c.is_ascii_digit());
            if is_version_shape && !SUPPORTED.contains(&version) {
                return FallbackKind::UnsupportedVersion;
            }
        }
    }
    FallbackKind::NotFound
}

/// Router fallback: structured 404s for API paths.
pub async fn api_fallback(request: Request) -> Response {
    let path = request.uri().path().to_string();
    match classify_path(&path) {
        FallbackKind::UnsupportedVersion => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "unsupported_api_version",
                "message": format!("this deployment does not serve the API version in {path}"),
                "current": CURRENT,
                "supported": SUPPORTED,
            })),
        )
            .into_response(),
        FallbackKind::NotFound => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" }))).into_response()
        }
    }
}

/// Middleware: stamp `API-Version: v1` on `/api/v1` responses.
pub async fn version_middleware(request: Request, next: Next) -> Response {
    let is_v1 = request.uri().path().starts_with("/api/v1");
    let mut response = next.run(request).await;
    if is_v1 {
        response
            .headers_mut()
            .insert(VERSION_HEADER, HeaderValue::from_static(CURRENT));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_versions_are_classified() {
        assert_eq!(
            classify_path("/api/v2/merchants"),
            FallbackKind::UnsupportedVersion
        );
        assert_eq!(
            classify_path("/api/v99/x"),
            FallbackKind::UnsupportedVersion
        );
        assert_eq!(classify_path("/api/v1/merchants"), FallbackKind::NotFound);
        assert_eq!(classify_path("/api/v1/typo"), FallbackKind::NotFound);
        assert_eq!(classify_path("/api/versions"), FallbackKind::NotFound);
        assert_eq!(classify_path("/health"), FallbackKind::NotFound);
        assert_eq!(classify_path("/api/vendors"), FallbackKind::NotFound);
    }
}
