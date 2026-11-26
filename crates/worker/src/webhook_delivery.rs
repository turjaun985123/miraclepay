//! Webhook delivery engine for MiraclePay v0.0.1.
//!
//! Clean-room implementation.
//!
//! This module performs no real HTTP itself. The actual transport is
//! injected through the [`HttpTransport`] trait, so the delivery policy
//! — signing, headers, status transitions, retry scheduling with
//! exponential backoff, and dead-lettering — can be unit-tested with a
//! mock transport and no network at all. A production caller supplies a
//! transport backed by a real HTTP client.
//!
//! Retry policy is not re-implemented here: the attempt limit comes from
//! [`miraclepay_domain::webhook::max_attempts`] and the delay before a
//! retry from [`miraclepay_domain::webhook::backoff_seconds`], so this
//! engine and the domain layer can never disagree about either number.
//!
//! # Transport trait shape
//!
//! `miraclepay-worker` deliberately does not depend on `async-trait`,
//! so [`HttpTransport::post_json`] uses the boxed-future pattern: it
//! returns a `Pin<Box<dyn Future + Send>>` instead of being declared
//! `async fn`. Implementors write an ordinary `async move` block and
//! box it, e.g. `Box::pin(async move { Ok(200) })`.

use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Duration, Utc};
use miraclepay_domain::webhook::{self, DeliveryStatus, WebhookDelivery, WebhookEvent};

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// Minimal outbound HTTP transport used to deliver one webhook.
///
/// Implementations POST `body` (already-serialised JSON) to `url` with
/// exactly the supplied `headers` and resolve to the HTTP status code
/// of the response. Transport-level failures — DNS, connection refused,
/// timeouts, TLS errors, or anything else where no HTTP response was
/// received — are reported as `Err` with a human-readable message; the
/// delivery engine treats `Err` and a non-2xx status the same way for
/// retry purposes, differing only in `last_status_code`.
pub trait HttpTransport: Send + Sync {
    /// POSTs `body` as JSON to `url` with `headers`.
    ///
    /// Returns the HTTP status code on any HTTP response (including
    /// error statuses such as 500), or `Err` when no response arrived.
    fn post_json<'a>(
        &'a self,
        url: &'a str,
        body: &'a str,
        headers: &'a [(String, String)],
    ) -> Pin<Box<dyn Future<Output = Result<u16, String>> + Send + 'a>>;
}

// ---------------------------------------------------------------------------
// Job and outcome
// ---------------------------------------------------------------------------

/// Everything needed to attempt delivery of one event to one endpoint.
///
/// The signing `secret` lives on the job rather than on
/// [`WebhookDelivery`] (whose persisted shape has no secret field), so
/// secrets never need to be stored alongside delivery bookkeeping.
#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryJob {
    /// The event being delivered; serialised as the request body.
    pub event: WebhookEvent,
    /// Destination endpoint URL.
    pub url: String,
    /// Webhook signing secret for this endpoint / merchant.
    pub secret: String,
}

/// Result of a single delivery attempt, mirroring the status the
/// [`WebhookDelivery`] was left in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeliveryOutcome {
    /// The endpoint answered 2xx; the delivery is complete.
    Delivered,
    /// The attempt failed but attempts remain; a retry is scheduled
    /// at `WebhookDelivery::next_retry_at`.
    RetryScheduled,
    /// The attempt failed and the attempt limit was reached; the
    /// delivery is dead-lettered and will not be retried automatically.
    DeadLetter,
}

// ---------------------------------------------------------------------------
// Single attempt
// ---------------------------------------------------------------------------

/// Attempts one delivery of `job`, mutating `delivery` in place.
///
/// The request body is the JSON serialisation of the whole
/// [`WebhookEvent`] (which carries its `payload`), and it is signed
/// with [`webhook::sign_payload`] keyed by `job.secret`. The transport
/// receives exactly these headers:
///
/// - `content-type: application/json`
/// - `x-miraclepay-signature` — hex HMAC-SHA256 of the body bytes
/// - `x-miraclepay-event` — `job.event.event_type`
/// - `x-miraclepay-delivery` — `delivery.id`
///
/// Outcomes:
///
/// - **2xx response:** `attempts` is incremented, `last_status_code`
///   records the code, `next_retry_at` is cleared, status becomes
///   [`DeliveryStatus::Delivered`], and [`DeliveryOutcome::Delivered`]
///   is returned.
/// - **Non-2xx response or transport error:** `attempts` is
///   incremented and `last_status_code` records the code (`None` when
///   no response was received). If `attempts` has reached
///   [`webhook::max_attempts`], status becomes
///   [`DeliveryStatus::DeadLetter`], no retry is scheduled, and
///   [`DeliveryOutcome::DeadLetter`] is returned. Otherwise status
///   becomes [`DeliveryStatus::Failed`],
///   `next_retry_at = now + backoff_seconds(attempts)` (using the
///   already-incremented attempt count), and
///   [`DeliveryOutcome::RetryScheduled`] is returned.
///
/// `now` is injected rather than read from the clock so retry times
/// are deterministic in tests and in batch processing.
pub async fn attempt_delivery(
    transport: &dyn HttpTransport,
    job: &DeliveryJob,
    delivery: &mut WebhookDelivery,
    now: DateTime<Utc>,
) -> DeliveryOutcome {
    // Serialising a `WebhookEvent` (whose payload is a
    // `serde_json::Value`) cannot fail in practice; the `expect`
    // documents that invariant rather than handling a reachable error.
    let body = serde_json::to_string(&job.event).expect("WebhookEvent serialises to JSON");
    let signature = webhook::sign_payload(&job.secret, body.as_bytes());
    // Wave 32: v2 timestamped signature alongside v1 during the
    // transition (receivers verify whichever they support; v2 adds
    // replay protection via the timestamp window).
    let signature_v2 = webhook::sign_payload_v2(&job.secret, now.timestamp(), body.as_bytes());

    let headers: Vec<(String, String)> = vec![
        ("content-type".to_string(), "application/json".to_string()),
        ("x-miraclepay-signature".to_string(), signature),
        ("x-miraclepay-signature-v2".to_string(), signature_v2),
        (
            "x-miraclepay-event".to_string(),
            job.event.event_type.clone(),
        ),
        ("x-miraclepay-delivery".to_string(), delivery.id.clone()),
    ];

    let result = transport.post_json(&job.url, &body, &headers).await;

    delivery.attempts += 1;

    match result {
        Ok(code) if (200..300).contains(&code) => {
            delivery.status = DeliveryStatus::Delivered;
            delivery.last_status_code = Some(code);
            delivery.next_retry_at = None;
            DeliveryOutcome::Delivered
        }
        Ok(code) => {
            delivery.last_status_code = Some(code);
            schedule_retry_or_dead_letter(delivery, now)
        }
        Err(_) => {
            // No HTTP response was received, so there is no status
            // code to record for this attempt.
            delivery.last_status_code = None;
            schedule_retry_or_dead_letter(delivery, now)
        }
    }
}

/// Shared failure tail of [`attempt_delivery`]: dead-letter once the
/// attempt limit is reached, otherwise schedule the next retry.
///
/// `delivery.attempts` must already have been incremented for the
/// attempt that just failed.
fn schedule_retry_or_dead_letter(
    delivery: &mut WebhookDelivery,
    now: DateTime<Utc>,
) -> DeliveryOutcome {
    if delivery.attempts >= webhook::max_attempts() {
        delivery.status = DeliveryStatus::DeadLetter;
        delivery.next_retry_at = None;
        DeliveryOutcome::DeadLetter
    } else {
        delivery.status = DeliveryStatus::Failed;
        let delay = webhook::backoff_seconds(delivery.attempts);
        delivery.next_retry_at = Some(now + Duration::seconds(delay as i64));
        DeliveryOutcome::RetryScheduled
    }
}

// ---------------------------------------------------------------------------
// Batch processing
// ---------------------------------------------------------------------------

/// Returns `true` when `delivery` is due for an attempt at `now`:
/// it is [`DeliveryStatus::Pending`] or [`DeliveryStatus::Failed`]
/// and either no retry is scheduled or the scheduled time has arrived
/// (`next_retry_at <= now`). Terminal deliveries
/// ([`DeliveryStatus::Delivered`] / [`DeliveryStatus::DeadLetter`])
/// are never due.
fn is_due(delivery: &WebhookDelivery, now: DateTime<Utc>) -> bool {
    if !matches!(
        delivery.status,
        DeliveryStatus::Pending | DeliveryStatus::Failed
    ) {
        return false;
    }
    match delivery.next_retry_at {
        None => true,
        Some(next_retry_at) => next_retry_at <= now,
    }
}

/// Attempts every due delivery in `jobs`, in order.
///
/// Each `(job, delivery)` pair whose delivery [`is_due`] at `now` is
/// attempted once via [`attempt_delivery`]; pairs that are not due
/// (terminal, or waiting for a future retry time) are left untouched
/// and produce no outcome. The returned outcomes are in the order the
/// attempts were made, one per attempted delivery, so callers can zip
/// them against the due subset — or simply count them by kind.
pub async fn process_due(
    transport: &dyn HttpTransport,
    jobs: &mut [(DeliveryJob, WebhookDelivery)],
    now: DateTime<Utc>,
) -> Vec<DeliveryOutcome> {
    let mut outcomes = Vec::new();
    for (job, delivery) in jobs.iter_mut() {
        if is_due(delivery, now) {
            outcomes.push(attempt_delivery(transport, job, delivery, now).await);
        }
    }
    outcomes
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    const SECRET: &str = "whsec_test_secret";
    const URL: &str = "https://merchant.example/webhooks";

    /// One transport call, captured for assertions.
    #[derive(Debug, Clone, PartialEq)]
    struct RecordedCall {
        url: String,
        body: String,
        headers: Vec<(String, String)>,
    }

    impl RecordedCall {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        }
    }

    /// Transport stub: always resolves to the configured result and
    /// records every call it receives.
    struct MockTransport {
        result: Result<u16, String>,
        calls: Mutex<Vec<RecordedCall>>,
    }

    impl MockTransport {
        fn with_status(status: u16) -> Self {
            Self {
                result: Ok(status),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn with_error(message: &str) -> Self {
            Self {
                result: Err(message.to_string()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl HttpTransport for MockTransport {
        fn post_json<'a>(
            &'a self,
            url: &'a str,
            body: &'a str,
            headers: &'a [(String, String)],
        ) -> Pin<Box<dyn Future<Output = Result<u16, String>> + Send + 'a>> {
            // Record synchronously, before returning the future, so
            // the call is captured even if assertions run after a
            // single `.await`.
            self.calls.lock().unwrap().push(RecordedCall {
                url: url.to_string(),
                body: body.to_string(),
                headers: headers.to_vec(),
            });
            Box::pin(async move { self.result.clone() })
        }
    }

    fn now() -> DateTime<Utc> {
        // A fixed instant keeps backoff assertions exact.
        DateTime::parse_from_rfc3339("2026-11-26T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn event() -> WebhookEvent {
        WebhookEvent {
            id: "evt_1".to_string(),
            merchant_id: "merch_1".to_string(),
            event_type: "payment.completed".to_string(),
            payload: json!({ "payment_id": "pay_1", "amount": 5000, "currency": "BDT" }),
            created_at: now(),
        }
    }

    fn job() -> DeliveryJob {
        DeliveryJob {
            event: event(),
            url: URL.to_string(),
            secret: SECRET.to_string(),
        }
    }

    fn delivery() -> WebhookDelivery {
        WebhookDelivery {
            id: "dlv_1".to_string(),
            event_id: "evt_1".to_string(),
            url: URL.to_string(),
            status: DeliveryStatus::Pending,
            attempts: 0,
            last_status_code: None,
            next_retry_at: None,
        }
    }

    // -- Success path ----------------------------------------------------

    #[tokio::test]
    async fn success_marks_delivered_and_counts_the_attempt() {
        let transport = MockTransport::with_status(200);
        let job = job();
        let mut delivery = delivery();

        let outcome = attempt_delivery(&transport, &job, &mut delivery, now()).await;

        assert_eq!(outcome, DeliveryOutcome::Delivered);
        assert_eq!(delivery.status, DeliveryStatus::Delivered);
        assert_eq!(delivery.attempts, 1);
        assert_eq!(delivery.last_status_code, Some(200));
        assert_eq!(delivery.next_retry_at, None);

        let calls = transport.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].url, URL);
        // The body is the serialised event, payload included.
        let sent: WebhookEvent = serde_json::from_str(&calls[0].body).unwrap();
        assert_eq!(sent, event());
        assert_eq!(calls[0].header("content-type"), Some("application/json"));
        assert_eq!(
            calls[0].header("x-miraclepay-event"),
            Some("payment.completed")
        );
        assert_eq!(calls[0].header("x-miraclepay-delivery"), Some("dlv_1"));
    }

    #[tokio::test]
    async fn any_2xx_status_counts_as_delivered() {
        for status in [200u16, 201, 204, 299] {
            let transport = MockTransport::with_status(status);
            let mut delivery = delivery();
            let outcome = attempt_delivery(&transport, &job(), &mut delivery, now()).await;
            assert_eq!(outcome, DeliveryOutcome::Delivered, "status {status}");
            assert_eq!(delivery.last_status_code, Some(status));
        }
    }

    // -- Signature ---------------------------------------------------------

    #[tokio::test]
    async fn signature_header_is_present_and_matches_sign_payload() {
        let transport = MockTransport::with_status(200);
        let mut delivery = delivery();

        attempt_delivery(&transport, &job(), &mut delivery, now()).await;

        let calls = transport.calls();
        assert_eq!(calls.len(), 1);
        let signature = calls[0]
            .header("x-miraclepay-signature")
            .expect("signature header must be present");
        let expected = webhook::sign_payload(SECRET, calls[0].body.as_bytes());
        assert_eq!(signature, expected);
        assert!(
            webhook::verify_signature(SECRET, calls[0].body.as_bytes(), signature),
            "receiver-side verification must accept the sent signature"
        );
    }

    // -- Retry scheduling ----------------------------------------------------

    #[tokio::test]
    async fn http_failure_schedules_retry_with_backoff() {
        let transport = MockTransport::with_status(500);
        let mut delivery = delivery();
        let start = now();

        let outcome = attempt_delivery(&transport, &job(), &mut delivery, start).await;

        assert_eq!(outcome, DeliveryOutcome::RetryScheduled);
        assert_eq!(delivery.status, DeliveryStatus::Failed);
        assert_eq!(delivery.attempts, 1);
        assert_eq!(delivery.last_status_code, Some(500));
        // After the increment, attempts == 1, so the delay is
        // backoff_seconds(1).
        let expected_delay = webhook::backoff_seconds(1);
        assert_eq!(expected_delay, 120);
        assert_eq!(
            delivery.next_retry_at,
            Some(start + Duration::seconds(expected_delay as i64))
        );
    }

    #[tokio::test]
    async fn transport_error_schedules_retry_without_a_status_code() {
        let transport = MockTransport::with_error("connection refused");
        let mut delivery = delivery();
        let start = now();

        let outcome = attempt_delivery(&transport, &job(), &mut delivery, start).await;

        assert_eq!(outcome, DeliveryOutcome::RetryScheduled);
        assert_eq!(delivery.status, DeliveryStatus::Failed);
        assert_eq!(delivery.attempts, 1);
        assert_eq!(delivery.last_status_code, None);
        assert_eq!(
            delivery.next_retry_at,
            Some(start + Duration::seconds(webhook::backoff_seconds(1) as i64))
        );
    }

    #[tokio::test]
    async fn backoff_grows_with_each_failed_attempt() {
        let transport = MockTransport::with_status(503);
        let mut delivery = delivery();
        let start = now();

        attempt_delivery(&transport, &job(), &mut delivery, start).await;
        let first_retry_at = delivery.next_retry_at.unwrap();

        attempt_delivery(&transport, &job(), &mut delivery, start).await;

        assert_eq!(delivery.attempts, 2);
        assert_eq!(
            delivery.next_retry_at,
            Some(start + Duration::seconds(webhook::backoff_seconds(2) as i64))
        );
        assert!(delivery.next_retry_at.unwrap() > first_retry_at);
    }

    // -- Dead letter ---------------------------------------------------------

    #[tokio::test]
    async fn failure_on_the_last_allowed_attempt_dead_letters() {
        let transport = MockTransport::with_status(500);
        let mut delivery = delivery();
        // One attempt short of the limit: this failure reaches it.
        delivery.attempts = webhook::max_attempts() - 1;
        delivery.status = DeliveryStatus::Failed;

        let outcome = attempt_delivery(&transport, &job(), &mut delivery, now()).await;

        assert_eq!(outcome, DeliveryOutcome::DeadLetter);
        assert_eq!(delivery.status, DeliveryStatus::DeadLetter);
        assert_eq!(delivery.attempts, webhook::max_attempts());
        assert_eq!(delivery.last_status_code, Some(500));
        assert_eq!(delivery.next_retry_at, None);
    }

    #[tokio::test]
    async fn repeated_failures_dead_letter_exactly_at_max_attempts() {
        let transport = MockTransport::with_error("timeout");
        let mut delivery = delivery();

        for attempt in 1..webhook::max_attempts() {
            let outcome = attempt_delivery(&transport, &job(), &mut delivery, now()).await;
            assert_eq!(
                outcome,
                DeliveryOutcome::RetryScheduled,
                "attempt {attempt}"
            );
            assert_eq!(delivery.status, DeliveryStatus::Failed);
        }
        let outcome = attempt_delivery(&transport, &job(), &mut delivery, now()).await;

        assert_eq!(outcome, DeliveryOutcome::DeadLetter);
        assert_eq!(delivery.attempts, webhook::max_attempts());
        assert_eq!(delivery.status, DeliveryStatus::DeadLetter);
        assert_eq!(transport.calls().len(), webhook::max_attempts() as usize);
    }

    // -- process_due -----------------------------------------------------------

    #[tokio::test]
    async fn process_due_attempts_only_due_deliveries() {
        let transport = MockTransport::with_status(200);
        let start = now();

        let mut pending = delivery();
        pending.id = "dlv_pending".to_string();

        let mut retry_due = delivery();
        retry_due.id = "dlv_retry_due".to_string();
        retry_due.status = DeliveryStatus::Failed;
        retry_due.attempts = 1;
        retry_due.next_retry_at = Some(start - Duration::seconds(1));

        let mut retry_not_due = delivery();
        retry_not_due.id = "dlv_retry_not_due".to_string();
        retry_not_due.status = DeliveryStatus::Failed;
        retry_not_due.attempts = 1;
        retry_not_due.next_retry_at = Some(start + Duration::seconds(600));

        let mut already_delivered = delivery();
        already_delivered.id = "dlv_done".to_string();
        already_delivered.status = DeliveryStatus::Delivered;
        already_delivered.attempts = 1;

        let mut dead = delivery();
        dead.id = "dlv_dead".to_string();
        dead.status = DeliveryStatus::DeadLetter;
        dead.attempts = webhook::max_attempts();

        let mut jobs: Vec<(DeliveryJob, WebhookDelivery)> = vec![
            (job(), pending),
            (job(), retry_due),
            (job(), retry_not_due),
            (job(), already_delivered),
            (job(), dead),
        ];

        let outcomes = process_due(&transport, &mut jobs, start).await;

        assert_eq!(
            outcomes,
            vec![DeliveryOutcome::Delivered, DeliveryOutcome::Delivered]
        );
        assert_eq!(transport.calls().len(), 2);

        assert_eq!(jobs[0].1.status, DeliveryStatus::Delivered);
        assert_eq!(jobs[1].1.status, DeliveryStatus::Delivered);
        // Untouched pairs keep their exact prior state.
        assert_eq!(jobs[2].1.status, DeliveryStatus::Failed);
        assert_eq!(jobs[2].1.attempts, 1);
        assert_eq!(
            jobs[2].1.next_retry_at,
            Some(start + Duration::seconds(600))
        );
        assert_eq!(jobs[3].1.status, DeliveryStatus::Delivered);
        assert_eq!(jobs[3].1.attempts, 1);
        assert_eq!(jobs[4].1.status, DeliveryStatus::DeadLetter);
    }

    #[tokio::test]
    async fn process_due_with_nothing_due_attempts_nothing() {
        let transport = MockTransport::with_status(200);
        let mut jobs: Vec<(DeliveryJob, WebhookDelivery)> = Vec::new();

        let outcomes = process_due(&transport, &mut jobs, now()).await;

        assert!(outcomes.is_empty());
        assert!(transport.calls().is_empty());
    }
}
