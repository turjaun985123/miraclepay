//! Fixed-window rate limiting for the MiraclePay API (Wave 12).
//!
//! A small in-memory limiter: per key (client address + route
//! class) it counts requests inside a 60-second window and rejects
//! with HTTP 429 once the class budget is spent.
//!
//! Route classes and budgets (per key, per minute):
//! - **auth** (`/api/v1/auth/*`) — 10: credential endpoints are the
//!   brute-force target, so they get the tightest budget.
//! - **sms** (`POST /api/v1/sms`) — 120: a merchant phone can burst
//!   legitimate payment SMS, but a runaway device must not flood.
//! - **api** (everything else under `/api/`) — 600.
//! - Non-API paths (health, docs) are unlimited.
//!
//! Client identity comes from the first `X-Forwarded-For` hop when
//! present (the API normally sits behind a proxy / the frontend
//! host), else the literal `"direct"`. A production deployment that
//! exposes the API directly should additionally rate-limit at its
//! edge proxy; this limiter is the in-process backstop.
//!
//! The window logic is a pure function of an injected clock
//! ([`RateLimiter::check_at`]) so it is unit-testable without
//! sleeping.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Window length for every route class.
pub const WINDOW: Duration = Duration::from_secs(60);
/// Budget for the auth class, per key per window.
pub const AUTH_LIMIT: u32 = 10;
/// Budget for the SMS ingest class, per key per window.
pub const SMS_LIMIT: u32 = 120;
/// Budget for the general API class, per key per window.
pub const API_LIMIT: u32 = 600;

/// Which budget a request path falls under (`None` = unlimited).
pub fn classify(path: &str, method: &str) -> Option<(&'static str, u32)> {
    if path.starts_with("/api/v1/auth/") {
        Some(("auth", AUTH_LIMIT))
    } else if path == "/api/v1/sms" && method.eq_ignore_ascii_case("POST") {
        Some(("sms", SMS_LIMIT))
    } else if path.starts_with("/api/") {
        Some(("api", API_LIMIT))
    } else {
        None
    }
}

#[derive(Debug)]
struct Bucket {
    window_start: Instant,
    count: u32,
}

/// The shared limiter state. Cheap to share behind an `Arc` (it is
/// stored on `AppState`).
#[derive(Debug, Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    /// A fresh, empty limiter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check (and record) one request for `key` under `limit` at
    /// time `now`. Returns `true` when the request is allowed.
    ///
    /// A bucket whose window has expired is reset before counting,
    /// so callers recover automatically at the window boundary.
    pub fn check_at(&self, key: &str, limit: u32, now: Instant) -> bool {
        let mut buckets = self.buckets.lock().expect("rate limiter lock");
        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            window_start: now,
            count: 0,
        });
        if now.duration_since(bucket.window_start) >= WINDOW {
            bucket.window_start = now;
            bucket.count = 0;
        }
        if bucket.count >= limit {
            return false;
        }
        bucket.count += 1;
        true
    }

    /// [`Self::check_at`] against the real clock.
    pub fn check(&self, key: &str, limit: u32) -> bool {
        self.check_at(key, limit, Instant::now())
    }
}

/// Axum middleware enforcing the limiter. Applied to the whole
/// router in `routes::build_router`; paths outside the classified
/// classes pass through untouched.
pub async fn rate_limit_middleware(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let (class, limit) = match classify(req.uri().path(), req.method().as_str()) {
        Some(pair) => pair,
        None => return next.run(req).await,
    };
    let client = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "direct".to_string());
    let key = format!("{class}:{client}");
    if state.rate_limiter.check(&key, limit) {
        next.run(req).await
    } else {
        (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            axum::Json(serde_json::json!({
                "error": "rate_limited",
                "message": "Too many requests; retry after the current minute window.",
                "class": class,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_routes_to_classes() {
        assert_eq!(classify("/api/v1/auth/login", "POST"), Some(("auth", AUTH_LIMIT)));
        assert_eq!(classify("/api/v1/sms", "POST"), Some(("sms", SMS_LIMIT)));
        assert_eq!(classify("/api/v1/sms", "GET"), Some(("api", API_LIMIT)));
        assert_eq!(classify("/api/v1/transactions", "GET"), Some(("api", API_LIMIT)));
        assert_eq!(classify("/health", "GET"), None);
    }

    #[test]
    fn allows_up_to_limit_then_blocks() {
        let limiter = RateLimiter::new();
        let now = Instant::now();
        for _ in 0..3 {
            assert!(limiter.check_at("auth:1.2.3.4", 3, now));
        }
        assert!(!limiter.check_at("auth:1.2.3.4", 3, now));
        // A different key has its own budget.
        assert!(limiter.check_at("auth:5.6.7.8", 3, now));
    }

    #[test]
    fn window_expiry_resets_the_budget() {
        let limiter = RateLimiter::new();
        let now = Instant::now();
        assert!(limiter.check_at("k", 1, now));
        assert!(!limiter.check_at("k", 1, now));
        let later = now + WINDOW + Duration::from_secs(1);
        assert!(limiter.check_at("k", 1, later));
    }
}
