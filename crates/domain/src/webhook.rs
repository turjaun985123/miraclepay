//! Webhook events, deliveries, signing, and retry policy for MiraclePay.
//!
//! Clean-room implementation.
//!
//! A [`WebhookEvent`] is something that happened in the system (for example
//! `payment.completed`) that a merchant wants to be told about. A
//! [`WebhookDelivery`] is a single logical attempt series to POST one event
//! to one merchant endpoint: it tracks how many attempts have been made,
//! what the endpoint last answered, and when the next retry is due.
//!
//! Outbound payloads are authenticated with an HMAC-SHA256 signature
//! (see [`sign_payload`] / [`verify_signature`]) so receivers can prove a
//! delivery really came from MiraclePay and was not altered in transit.
//! Failed deliveries are retried with exponential backoff (see
//! [`backoff_seconds`]) up to [`max_attempts`] attempts, after which a
//! delivery should be moved to [`DeliveryStatus::DeadLetter`] for manual
//! inspection instead of being retried forever.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// HMAC-SHA256 keyed with a merchant's webhook signing secret.
type HmacSha256 = Hmac<Sha256>;

/// Maximum backoff between delivery attempts, in seconds (24 hours).
const MAX_BACKOFF_SECONDS: u64 = 86_400;

/// Backoff granted before the first retry, in seconds.
const BASE_BACKOFF_SECONDS: u64 = 60;

/// Total number of delivery attempts (initial attempt plus retries)
/// before a delivery is eligible for dead-lettering.
const MAX_ATTEMPTS: u32 = 8;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// An event to be delivered to a merchant's webhook endpoint.
///
/// `event_type` is a dotted, lowercase name such as `"payment.completed"`,
/// `"payment.failed"`, or `"payment.refunded"`. `payload` is the
/// event-specific JSON body that is serialised and sent to the endpoint;
/// it is kept as an untyped [`serde_json::Value`] because each event type
/// carries a different shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebhookEvent {
    /// Unique identifier for this event.
    pub id: String,
    /// Identifier of the merchant this event belongs to.
    pub merchant_id: String,
    /// Dotted event name, e.g. `"payment.completed"`.
    pub event_type: String,
    /// Event-specific JSON payload delivered to the endpoint.
    pub payload: serde_json::Value,
    /// When the event was created.
    pub created_at: DateTime<Utc>,
}

/// Lifecycle state of a [`WebhookDelivery`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Not yet delivered; awaiting its first attempt or a retry.
    Pending,
    /// The endpoint acknowledged the delivery (a 2xx response).
    Delivered,
    /// The most recent attempt failed, but retries may still follow.
    Failed,
    /// All attempts were exhausted; no further automatic retries.
    DeadLetter,
}

impl DeliveryStatus {
    /// Returns `true` when no further automatic delivery attempts will
    /// be made in this state (`Delivered` or `DeadLetter`).
    pub fn is_terminal(&self) -> bool {
        matches!(self, DeliveryStatus::Delivered | DeliveryStatus::DeadLetter)
    }
}

/// One logical delivery of a [`WebhookEvent`] to a single URL.
///
/// A delivery aggregates every HTTP attempt made for that event/URL pair:
/// `attempts` counts the attempts performed so far,
/// `last_status_code` records the HTTP status of the most recent attempt
/// (`None` before the first attempt, or when the attempt never received
/// an HTTP response, e.g. a connection error), and `next_retry_at` is
/// when the next attempt becomes due (`None` when no retry is scheduled).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebhookDelivery {
    /// Unique identifier for this delivery.
    pub id: String,
    /// Identifier of the [`WebhookEvent`] being delivered.
    pub event_id: String,
    /// Destination endpoint URL.
    pub url: String,
    /// Current lifecycle state.
    pub status: DeliveryStatus,
    /// Number of delivery attempts performed so far.
    pub attempts: u32,
    /// HTTP status code of the most recent attempt, if one was received.
    pub last_status_code: Option<u16>,
    /// When the next retry becomes due, if one is scheduled.
    pub next_retry_at: Option<DateTime<Utc>>,
}

impl WebhookDelivery {
    /// Returns `true` while this delivery may still be attempted:
    /// it is not in a terminal state and has attempts remaining.
    pub fn is_retryable(&self) -> bool {
        !self.status.is_terminal() && self.attempts < max_attempts()
    }
}

// ---------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------

/// Computes the hex-encoded HMAC-SHA256 of `payload` keyed by `secret`.
///
/// The returned string is 64 lowercase hexadecimal characters. Receivers
/// compare it against the signature header sent with the delivery using
/// [`verify_signature`].
///
/// The unit tests in this module exercise this function directly,
/// including a known-answer HMAC-SHA256 vector.
pub fn sign_payload(secret: &str, payload: &[u8]) -> String {
    let mac = mac_for(secret, payload);
    hex_encode(&mac.finalize().into_bytes())
}

/// Verifies a hex-encoded HMAC-SHA256 `signature_hex` for `payload`
/// against `secret`.
///
/// Returns `false` — never panics — for a malformed signature (odd
/// length, non-hex characters, or the wrong byte length) as well as for
/// a well-formed signature that does not match. The comparison itself
/// is performed by the `hmac` crate's `verify_slice`, which compares in
/// constant time, so a mismatch does not leak how many leading bytes
/// were correct.
pub fn verify_signature(secret: &str, payload: &[u8], signature_hex: &str) -> bool {
    let Some(signature) = hex_decode(signature_hex) else {
        return false;
    };
    mac_for(secret, payload).verify_slice(&signature).is_ok()
}

/// Builds an HMAC-SHA256 instance keyed by `secret` and fed `payload`.
///
/// HMAC accepts keys of any length, so constructing the MAC from a
/// `&str` secret cannot fail in practice; the `expect` documents that
/// invariant rather than handling a reachable error.
fn mac_for(secret: &str, payload: &[u8]) -> HmacSha256 {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(payload);
    mac
}

// ---------------------------------------------------------------------------
// Retry policy
// ---------------------------------------------------------------------------

/// Seconds to wait before retry number `attempt` (0-based).
///
/// The delay doubles on every attempt, starting from 60 seconds:
///
/// | attempt | delay (seconds) |
/// |---------|-----------------|
/// | 0       | 60              |
/// | 1       | 120             |
/// | 2       | 240             |
/// | 3       | 480             |
/// | 4       | 960             |
/// | 5       | 1,920           |
/// | 6       | 3,840           |
/// | 7       | 7,680           |
/// | 10      | 61,440          |
/// | 11+     | 86,400 (capped) |
///
/// The result is capped at 86,400 seconds (24 hours), and arbitrarily
/// large `attempt` values saturate at the cap instead of overflowing.
pub fn backoff_seconds(attempt: u32) -> u64 {
    let factor = 2u64.checked_pow(attempt).unwrap_or(u64::MAX);
    BASE_BACKOFF_SECONDS
        .saturating_mul(factor)
        .min(MAX_BACKOFF_SECONDS)
}

/// Maximum number of delivery attempts for a single webhook delivery,
/// including the initial attempt. After this many attempts a delivery
/// should be transitioned to [`DeliveryStatus::DeadLetter`].
pub fn max_attempts() -> u32 {
    MAX_ATTEMPTS
}

// ---------------------------------------------------------------------------
// Hex helpers (kept local so this module needs no `hex` crate)
// ---------------------------------------------------------------------------

/// Encodes bytes as lowercase hexadecimal.
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Decodes a hexadecimal string, accepting upper- or lowercase digits.
///
/// Returns `None` for an odd number of characters or any non-hex digit.
fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        out.push((high << 4) | low);
    }
    Some(out)
}

/// Returns the numeric value of a single ASCII hex digit.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SECRET: &str = "whsec_test_secret";
    const PAYLOAD: &[u8] = br#"{"id":"evt_1","type":"payment.completed","amount":5000}"#;

    // -- Signing -----------------------------------------------------------

    #[test]
    fn sign_verify_roundtrip() {
        let signature = sign_payload(SECRET, PAYLOAD);
        assert!(verify_signature(SECRET, PAYLOAD, &signature));
    }

    #[test]
    fn sign_produces_64_lowercase_hex_chars() {
        let signature = sign_payload(SECRET, PAYLOAD);
        assert_eq!(signature.len(), 64);
        assert!(
            signature
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "signature was {signature}"
        );
    }

    #[test]
    fn sign_matches_known_hmac_sha256_vector() {
        // Well-known HMAC-SHA256 test vector:
        // key "key", message "The quick brown fox jumps over the lazy dog".
        let signature = sign_payload("key", b"The quick brown fox jumps over the lazy dog");
        assert_eq!(
            signature,
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn sign_is_deterministic_and_secret_dependent() {
        assert_eq!(sign_payload(SECRET, PAYLOAD), sign_payload(SECRET, PAYLOAD));
        assert_ne!(
            sign_payload(SECRET, PAYLOAD),
            sign_payload("whsec_other_secret", PAYLOAD)
        );
        assert_ne!(
            sign_payload(SECRET, PAYLOAD),
            sign_payload(SECRET, b"{}")
        );
    }

    #[test]
    fn verify_rejects_tampered_payload() {
        let signature = sign_payload(SECRET, PAYLOAD);
        let mut tampered = PAYLOAD.to_vec();
        tampered[0] = b'[';
        assert!(!verify_signature(SECRET, &tampered, &signature));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let signature = sign_payload(SECRET, PAYLOAD);
        assert!(!verify_signature("whsec_wrong", PAYLOAD, &signature));
    }

    #[test]
    fn verify_rejects_malformed_signatures_without_panicking() {
        assert!(!verify_signature(SECRET, PAYLOAD, ""));
        assert!(!verify_signature(SECRET, PAYLOAD, "abc")); // odd length
        assert!(!verify_signature(SECRET, PAYLOAD, "zz".repeat(32).as_str()));
        // Well-formed hex, but the wrong length / wrong value.
        assert!(!verify_signature(SECRET, PAYLOAD, "00"));
        assert!(!verify_signature(SECRET, PAYLOAD, &"00".repeat(32)));
        assert!(!verify_signature(SECRET, PAYLOAD, &"00".repeat(64)));
    }

    #[test]
    fn verify_accepts_uppercase_hex() {
        let signature = sign_payload(SECRET, PAYLOAD).to_uppercase();
        assert!(verify_signature(SECRET, PAYLOAD, &signature));
    }

    #[test]
    fn sign_and_verify_empty_payload_and_secret() {
        let signature = sign_payload("", b"");
        assert!(verify_signature("", b"", &signature));
        assert!(!verify_signature("", b"not empty", &signature));
    }

    // -- Backoff -----------------------------------------------------------

    #[test]
    fn backoff_doubles_from_sixty_seconds() {
        let expected = [60, 120, 240, 480, 960, 1_920, 3_840, 7_680];
        for (attempt, &want) in expected.iter().enumerate() {
            assert_eq!(backoff_seconds(attempt as u32), want, "attempt {attempt}");
        }
    }

    #[test]
    fn backoff_is_capped_at_one_day() {
        // 60 * 2^10 = 61,440 is the last delay below the cap.
        assert_eq!(backoff_seconds(10), 61_440);
        assert_eq!(backoff_seconds(11), MAX_BACKOFF_SECONDS);
        assert_eq!(backoff_seconds(12), MAX_BACKOFF_SECONDS);
        assert_eq!(backoff_seconds(100), MAX_BACKOFF_SECONDS);
    }

    #[test]
    fn backoff_never_overflows_for_huge_attempts() {
        assert_eq!(backoff_seconds(u32::MAX), MAX_BACKOFF_SECONDS);
    }

    #[test]
    fn backoff_is_monotonically_non_decreasing() {
        let mut previous = 0;
        for attempt in 0..=64 {
            let current = backoff_seconds(attempt);
            assert!(current >= previous, "attempt {attempt}");
            previous = current;
        }
    }

    // -- Retry limit --------------------------------------------------------

    #[test]
    fn max_attempts_is_eight() {
        assert_eq!(max_attempts(), 8);
    }

    // -- Types ---------------------------------------------------------------

    #[test]
    fn delivery_status_terminal_states() {
        assert!(!DeliveryStatus::Pending.is_terminal());
        assert!(!DeliveryStatus::Failed.is_terminal());
        assert!(DeliveryStatus::Delivered.is_terminal());
        assert!(DeliveryStatus::DeadLetter.is_terminal());
    }

    #[test]
    fn delivery_status_serialises_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&DeliveryStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&DeliveryStatus::DeadLetter).unwrap(),
            "\"dead_letter\""
        );
        assert_eq!(
            serde_json::from_str::<DeliveryStatus>("\"delivered\"").unwrap(),
            DeliveryStatus::Delivered
        );
    }

    #[test]
    fn webhook_event_serde_roundtrip() {
        let event = WebhookEvent {
            id: "evt_1".to_string(),
            merchant_id: "merch_1".to_string(),
            event_type: "payment.completed".to_string(),
            payload: json!({ "payment_id": "pay_1", "amount": 5000, "currency": "BDT" }),
            created_at: Utc::now(),
        };
        let encoded = serde_json::to_string(&event).unwrap();
        let decoded: WebhookEvent = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn webhook_delivery_serde_roundtrip() {
        let delivery = WebhookDelivery {
            id: "dlv_1".to_string(),
            event_id: "evt_1".to_string(),
            url: "https://merchant.example/webhooks".to_string(),
            status: DeliveryStatus::Failed,
            attempts: 2,
            last_status_code: Some(500),
            next_retry_at: Some(Utc::now()),
        };
        let encoded = serde_json::to_string(&delivery).unwrap();
        let decoded: WebhookDelivery = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, delivery);
    }

    #[test]
    fn fresh_delivery_is_retryable_until_attempts_run_out() {
        let mut delivery = WebhookDelivery {
            id: "dlv_1".to_string(),
            event_id: "evt_1".to_string(),
            url: "https://merchant.example/webhooks".to_string(),
            status: DeliveryStatus::Pending,
            attempts: 0,
            last_status_code: None,
            next_retry_at: None,
        };
        assert!(delivery.is_retryable());

        delivery.attempts = max_attempts();
        assert!(!delivery.is_retryable());

        delivery.attempts = 1;
        delivery.status = DeliveryStatus::Delivered;
        assert!(!delivery.is_retryable());
    }
}
