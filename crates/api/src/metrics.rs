//! Observability (Wave 44): Prometheus metrics + structured
//! request logs.
//!
//! - `GET /metrics` renders the current store as Prometheus text
//!   exposition (no extra dependencies — the format is line-based
//!   and rendered by hand with proper label escaping). Metrics are
//!   computed on scrape from the live collections, so they can
//!   never drift from the data they describe.
//! - [`request_log_middleware`] writes one JSON line per request
//!   to stdout (ts, request id, method, path, status, latency) and
//!   echoes/propagates `x-request-id` so a client-visible id can be
//!   grepped straight to the log line.

use std::time::Instant;

use axum::extract::Request;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use miraclepay_domain::transaction::TxStatus;

use crate::state::AppState;

/// Escape a Prometheus label value.
fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// One exposition line: `name{labels} value`.
fn line(name: &str, labels: &[(&str, String)], value: i64) -> String {
    if labels.is_empty() {
        format!("{name} {value}\n")
    } else {
        let inner: Vec<String> = labels
            .iter()
            .map(|(key, val)| format!("{key}=\"{}\"", escape_label(val)))
            .collect();
        format!("{name}{{{}}} {value}\n", inner.join(","))
    }
}

/// Render the whole metrics document for the current state.
pub async fn render_metrics(state: &AppState) -> String {
    let mut out = String::new();
    out.push_str("# HELP miraclepay_uptime_seconds Seconds since this API process started.\n");
    out.push_str("# TYPE miraclepay_uptime_seconds gauge\n");
    out.push_str(&line(
        "miraclepay_uptime_seconds",
        &[],
        state.started_at.elapsed().as_secs() as i64,
    ));

    out.push_str(&line(
        "miraclepay_users_total",
        &[],
        state.users.read().await.len() as i64,
    ));
    out.push_str(&line(
        "miraclepay_merchants_total",
        &[],
        state.merchants.read().await.len() as i64,
    ));
    out.push_str(&line(
        "miraclepay_db_mode",
        &[("mode", state.db_mode().as_str().to_string())],
        1,
    ));

    let transactions = state.transactions.read().await;
    for status in [
        TxStatus::Pending,
        TxStatus::Processing,
        TxStatus::Completed,
        TxStatus::Failed,
        TxStatus::Refunded,
        TxStatus::PartiallyRefunded,
        TxStatus::Expired,
    ] {
        let count = transactions
            .values()
            .filter(|tx| tx.status == status)
            .count() as i64;
        out.push_str(&line(
            "miraclepay_transactions_total",
            &[("status", format!("{status:?}").to_lowercase())],
            count,
        ));
    }
    drop(transactions);

    let deliveries = state.deliveries.read().await;
    for status in ["pending", "delivered", "failed", "deadletter"] {
        let count = deliveries
            .iter()
            .filter(|delivery| format!("{:?}", delivery.status).to_lowercase() == status)
            .count() as i64;
        out.push_str(&line(
            "miraclepay_webhook_deliveries_total",
            &[("status", status.to_string())],
            count,
        ));
    }
    drop(deliveries);

    let disputes_open = state
        .disputes
        .read()
        .await
        .values()
        .filter(|dispute| !dispute.status.is_terminal())
        .count() as i64;
    out.push_str(&line("miraclepay_disputes_open", &[], disputes_open));

    let queued_mail = state
        .notifications
        .read()
        .await
        .iter()
        .filter(|n| {
            matches!(
                n.status,
                miraclepay_domain::notification::NotificationStatus::Queued
            )
        })
        .count() as i64;
    out.push_str(&line("miraclepay_notifications_queued", &[], queued_mail));
    out
}

/// `GET /metrics` — Prometheus text exposition.
pub async fn metrics_handler(State(state): State<AppState>) -> Response {
    let body = render_metrics(&state).await;
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

/// Middleware: structured JSON request log + `x-request-id`.
pub async fn request_log_middleware(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .unwrap_or_else(|| miraclepay_domain::ids::new_id("req"));
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let started = Instant::now();
    let mut response = next.run(request).await;
    let status = response.status().as_u16();
    let latency_ms = started.elapsed().as_millis();
    // One JSON line per request on stdout — greppable, parseable,
    // and free of bodies/headers (nothing sensitive is logged).
    println!(
        "{}",
        serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "request_id": request_id,
            "method": method.as_str(),
            "path": path,
            "status": status,
            "latency_ms": latency_ms,
        })
    );
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_escaping_is_prometheus_safe() {
        assert_eq!(escape_label("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn lines_render_with_and_without_labels() {
        assert_eq!(line("up", &[], 1), "up 1\n");
        assert_eq!(
            line("tx", &[("status", "completed".to_string())], 7),
            "tx{status=\"completed\"} 7\n"
        );
    }
}
