//! Unique identifier generation for MiraclePay v0.0.1.
//!
//! This module provides a small, dependency-free (std-only) way to mint
//! human-recognisable, prefixed identifiers such as `txn_...`, `mer_...`
//! or `evt_...` for the various entities in the domain.
//!
//! # Format
//!
//! Every identifier produced by [`new_id`] has the shape:
//!
//! ```text
//! {prefix}_{timestamp_hex}{counter_hex}{entropy_hex}
//! ```
//!
//! where:
//!
//! * `timestamp_hex` — the current [`SystemTime`] expressed as nanoseconds
//!   since the Unix epoch, encoded in lower-case hexadecimal. Because the
//!   timestamp comes first, identifiers generated later sort (lexicographically)
//!   after earlier ones *within the same process and for the same prefix*,
//!   assuming a monotonic system clock. Hence "sortable-ish": clock skew,
//!   NTP adjustments, and ids from different processes or prefixes can break
//!   strict ordering.
//! * `counter_hex` — a process-wide monotonically increasing counter
//!   (a `static` [`AtomicU64`]) encoded in lower-case hexadecimal. This is
//!   what guarantees uniqueness for ids minted at the same nanosecond, or
//!   even if the clock moves backwards: two calls in the same process never
//!   receive the same counter value.
//! * `entropy_hex` — a value derived by mixing the process id, the counter,
//!   and the timestamp through a simple integer avalanche mixer. It is
//!   *not* cryptographic entropy; it merely spreads bits so that ids from
//!   different processes started at similar times are very unlikely to
//!   collide, and so the suffix does not look trivially sequential.
//!
//! # Guarantees and non-guarantees
//!
//! * **Unique per process:** within a single process, ids are unique
//!   because the atomic counter never repeats (until it would wrap after
//!   2^64 calls, which is not a practical concern).
//! * **Sortable-ish:** ids are roughly ordered by creation time, as
//!   described above. Do not rely on them for strict total ordering.
//! * **Not a UUID:** the output is not an RFC 4122 UUID and does not claim
//!   to be.
//! * **Not a CSPRNG:** the entropy component is a deterministic mix of the
//!   process id, counter, and timestamp. Ids are therefore predictable to
//!   an observer who knows approximately when they were minted. Never use
//!   these identifiers as secrets, session tokens, password-reset tokens,
//!   or unguessable capability URLs. Use a dedicated cryptographically
//!   secure generator for anything security-sensitive.
//!
//! # Dependencies
//!
//! Standard library only: [`std::time::SystemTime`],
//! [`std::sync::atomic::AtomicU64`], and [`std::process::id`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Process-wide monotonically increasing counter backing [`new_id`].
///
/// Seeded lazily on first use (see [`next_counter`]) with a value mixed
/// from the process id and the current time, so two processes started at
/// roughly the same moment do not both start counting from zero.
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Whether [`COUNTER`] has been seeded yet.
static COUNTER_SEEDED: AtomicU64 = AtomicU64::new(0);

/// A small integer avalanche mixer (splitmix64-style finaliser).
///
/// Used only to spread bits for the entropy component and the counter
/// seed; it has no cryptographic properties.
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    x
}

/// Current time as nanoseconds since the Unix epoch.
///
/// Falls back to `0` if the system clock is set before the epoch, which
/// should never happen in practice; uniqueness is still preserved by the
/// counter in that degenerate case.
fn now_nanos() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as u64,
        Err(_) => 0,
    }
}

/// Returns the next counter value, seeding the counter on first use.
///
/// Seeding mixes the process id with the current timestamp so distinct
/// processes do not share the same starting point. The compare-exchange
/// dance keeps the seeding race benign: whichever thread wins installs
/// the seed, and every caller still receives a distinct value from the
/// subsequent `fetch_add`.
fn next_counter() -> u64 {
    if COUNTER_SEEDED.load(Ordering::Relaxed) == 0 {
        let seed = mix64((std::process::id() as u64) ^ now_nanos().rotate_left(17));
        // Best-effort seed install; losing the race is fine.
        let _ = COUNTER.compare_exchange(0, seed, Ordering::SeqCst, Ordering::SeqCst);
        COUNTER_SEEDED.store(1, Ordering::SeqCst);
    }
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Generates a new unique identifier with the given `prefix`.
///
/// The result has the form `"{prefix}_{timestamp_hex}{counter_hex}{entropy_hex}"`,
/// for example `new_id("txn")` might produce `"txn_18c2f1a9b3d4e5f6..."`.
/// The prefix is used verbatim; callers should pass a short lower-case
/// code (the convenience functions in this module use three-letter codes).
///
/// See the module documentation for the exact guarantees: unique per
/// process, sortable-ish by creation time, not a UUID, and not generated
/// from a CSPRNG — never use the result as a secret.
pub fn new_id(prefix: &str) -> String {
    let timestamp = now_nanos();
    let counter = next_counter();

    // Entropy: mix process id, counter, and timestamp so ids from
    // different processes diverge even when minted at the same instant.
    let entropy = mix64(
        (std::process::id() as u64)
            ^ counter.wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ timestamp.rotate_left(31),
    );

    format!("{prefix}_{timestamp:016x}{counter:016x}{entropy:016x}")
}

/// Generates a new transaction identifier (`"txn"` prefix).
pub fn transaction_id() -> String {
    new_id("txn")
}

/// Generates a new merchant identifier (`"mer"` prefix).
pub fn merchant_id() -> String {
    new_id("mer")
}

/// Generates a new invoice identifier (`"inv"` prefix).
pub fn invoice_id() -> String {
    new_id("inv")
}

/// Generates a new refund identifier (`"ref"` prefix).
pub fn refund_id() -> String {
    new_id("ref")
}

/// Generates a new settlement identifier (`"stl"` prefix).
pub fn settlement_id() -> String {
    new_id("stl")
}

/// Generates a new webhook event identifier (`"evt"` prefix).
pub fn webhook_event_id() -> String {
    new_id("evt")
}

/// Generates a new payment link identifier (`"lnk"` prefix).
pub fn payment_link_id() -> String {
    new_id("lnk")
}

/// Generates a new API key identifier (`"key"` prefix).
pub fn api_key_id() -> String {
    new_id("key")
}

/// Generates a new user identifier (`"usr"` prefix).
pub fn user_id() -> String {
    new_id("usr")
}

/// Generates a new audit record identifier (`"aud"` prefix).
pub fn audit_id() -> String {
    new_id("aud")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn new_id_uses_given_prefix() {
        let id = new_id("xyz");
        assert!(
            id.starts_with("xyz_"),
            "id {id:?} should start with \"xyz_\""
        );
    }

    #[test]
    fn convenience_functions_use_expected_prefixes() {
        // A table of (generator, prefix) pairs — the fn-pointer
        // type is the point of the test, so the complexity lint is
        // allowed here rather than factored away.
        #[allow(clippy::type_complexity)]
        let cases: [(fn() -> String, &str); 10] = [
            (transaction_id, "txn_"),
            (merchant_id, "mer_"),
            (invoice_id, "inv_"),
            (refund_id, "ref_"),
            (settlement_id, "stl_"),
            (webhook_event_id, "evt_"),
            (payment_link_id, "lnk_"),
            (api_key_id, "key_"),
            (user_id, "usr_"),
            (audit_id, "aud_"),
        ];

        for (make_id, prefix) in cases {
            let id = make_id();
            assert!(
                id.starts_with(prefix),
                "id {id:?} should start with {prefix:?}"
            );
        }
    }

    #[test]
    fn ids_are_unique_across_many_calls() {
        let mut seen: HashSet<String> = HashSet::with_capacity(1000);
        for _ in 0..1000 {
            let id = transaction_id();
            assert!(seen.insert(id.clone()), "duplicate id generated: {id}");
        }
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn ids_are_non_empty_and_have_non_empty_suffix() {
        for make_id in [
            transaction_id,
            merchant_id,
            invoice_id,
            refund_id,
            settlement_id,
            webhook_event_id,
            payment_link_id,
            api_key_id,
            user_id,
            audit_id,
        ] {
            let id = make_id();
            assert!(!id.is_empty(), "id should not be empty");

            let suffix = id
                .split_once('_')
                .map(|(_, rest)| rest)
                .expect("id should contain an underscore separator");
            assert!(!suffix.is_empty(), "id suffix should not be empty: {id:?}");
            assert!(
                suffix.chars().all(|c| c.is_ascii_hexdigit()),
                "id suffix should be hexadecimal: {id:?}"
            );
        }

        let custom = new_id("custom");
        assert!(!custom.is_empty());
        assert!(custom.len() > "custom_".len());
    }
}
