//! Security headers + CORS middleware (Wave 24).
//!
//! Applied as the outermost layer of the router, so every response
//! — including rate-limit 429s and error responses — carries the
//! headers:
//!
//! - `X-Content-Type-Options: nosniff`
//! - `X-Frame-Options: DENY` + CSP `frame-ancestors 'none'` (the API
//!   serves JSON only; it must never be framed)
//! - `Content-Security-Policy: default-src 'none'` (JSON API: no
//!   scripts, styles, or subresources of its own)
//! - `Referrer-Policy: no-referrer`
//! - `Strict-Transport-Security: max-age=31536000; includeSubDomains`
//!   (browsers ignore it over plain HTTP, so local dev is unaffected)
//!
//! CORS is **off by default**: the shipped deployment is same-origin
//! (the frontend is served alongside / proxied to the API). Set
//! `CORS_ALLOWED_ORIGINS` to a comma-separated origin allowlist
//! (e.g. `https://pay.example.com,https://admin.example.com`) to
//! enable cross-origin browser access; `*` is honoured but the
//! request origin is echoed back rather than a literal wildcard so
//! credentialed requests keep working. Allowed methods/headers
//! mirror the API surface (Authorization, Content-Type,
//! X-Device-Key).

use std::sync::OnceLock;

use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Parse the `CORS_ALLOWED_ORIGINS` env value: comma-separated,
/// trimmed, empties dropped.
pub fn parse_origins(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// The process-wide allowlist, read once from the environment.
pub fn allowed_origins() -> &'static [String] {
    static ORIGINS: OnceLock<Vec<String>> = OnceLock::new();
    ORIGINS.get_or_init(|| {
        std::env::var("CORS_ALLOWED_ORIGINS")
            .map(|raw| parse_origins(&raw))
            .unwrap_or_default()
    })
}

/// Whether `origin` is allowed by `allowlist` (exact match, or a
/// `*` entry). An empty allowlist allows nothing (CORS off).
pub fn origin_allowed(allowlist: &[String], origin: &str) -> bool {
    allowlist.iter().any(|entry| entry == "*" || entry == origin)
}

const ALLOW_METHODS: &str = "GET, POST, PUT, PATCH, DELETE, OPTIONS";
const ALLOW_HEADERS: &str = "authorization, content-type, x-device-key";

/// Outermost middleware: CORS preflight handling + security headers
/// on every response.
pub async fn security_middleware(request: Request, next: Next) -> Response {
    let origins = allowed_origins();
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let allowed_origin = origin
        .as_deref()
        .filter(|origin| origin_allowed(origins, origin))
        .map(str::to_string);

    // CORS preflight: answer directly, never reaching the handlers
    // (and never consuming rate-limit budget).
    let is_preflight = request.method() == Method::OPTIONS
        && request
            .headers()
            .contains_key("access-control-request-method");
    if is_preflight {
        let mut response = StatusCode::NO_CONTENT.into_response();
        if let Some(origin) = &allowed_origin {
            apply_cors_headers(&mut response, origin, true);
        }
        apply_security_headers(&mut response);
        return response;
    }

    let mut response = next.run(request).await;
    if let Some(origin) = &allowed_origin {
        apply_cors_headers(&mut response, origin, false);
    }
    apply_security_headers(&mut response);
    response
}

fn apply_cors_headers(response: &mut Response, origin: &str, preflight: bool) {
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(origin) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    if preflight {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static(ALLOW_METHODS),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(ALLOW_HEADERS),
        );
        headers.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
    }
}

fn apply_security_headers(response: &mut Response) {
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
    );
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_origins_trims_and_drops_empties() {
        assert_eq!(
            parse_origins(" https://a.test , ,https://b.test,"),
            vec!["https://a.test".to_string(), "https://b.test".to_string()]
        );
        assert!(parse_origins("").is_empty());
    }

    #[test]
    fn origin_allowed_requires_match_or_wildcard() {
        let list = vec!["https://a.test".to_string()];
        assert!(origin_allowed(&list, "https://a.test"));
        assert!(!origin_allowed(&list, "https://evil.test"));
        assert!(!origin_allowed(&[], "https://a.test"));
        assert!(origin_allowed(&["*".to_string()], "https://anything.test"));
    }
}
