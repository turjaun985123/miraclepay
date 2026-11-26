//! API key domain types for MiraclePay.
//!
//! Original clean-room implementation. An API key authenticates a
//! merchant's server-side integration. Only a hash of the raw key is
//! ever stored: the raw key is shown to the merchant exactly once, at
//! creation time, and cannot be recovered afterwards.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Prefix that begins every live raw API key.
pub const KEY_PREFIX: &str = "mp_live_";

/// Number of hex characters of entropy material in a raw API key,
/// after [`KEY_PREFIX`].
pub const KEY_HEX_LEN: usize = 32;

/// Number of leading characters of a raw key kept as its public,
/// non-secret prefix (safe to display and to look keys up by).
pub const DISPLAY_PREFIX_LEN: usize = 12;

/// A permission an API key can carry.
///
/// Scopes are coarse-grained on purpose: each one maps to a family of
/// API operations, and a key should be granted only the scopes its
/// integration actually needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiScope {
    /// Read payments (list / retrieve transactions and payment state).
    PaymentsRead,
    /// Create and act on payments.
    PaymentsWrite,
    /// Create and act on invoices.
    InvoicesWrite,
    /// Create, update, and delete webhook endpoints.
    WebhooksManage,
    /// Create refunds.
    RefundsWrite,
    /// Unrestricted access; implies every other scope.
    FullAccess,
}

impl ApiScope {
    /// Returns `true` if a key carrying `self` is allowed to perform
    /// an operation that requires the `required` scope.
    ///
    /// [`ApiScope::FullAccess`] allows everything. Every other scope
    /// allows only itself — in particular, a write scope does not
    /// implicitly grant the corresponding read scope; grant both if
    /// both are needed.
    pub fn allows(&self, required: ApiScope) -> bool {
        match self {
            ApiScope::FullAccess => true,
            scope => *scope == required,
        }
    }
}

/// An API key belonging to a merchant.
///
/// `key_hash` is the hex SHA-256 of the raw key (see [`hash_key`]);
/// the raw key itself is never stored. `key_prefix` is the first
/// [`DISPLAY_PREFIX_LEN`] characters of the raw key, kept so keys can
/// be identified and looked up without exposing the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique API key identifier.
    pub id: String,
    /// Identifier of the merchant this key belongs to.
    pub merchant_id: String,
    /// Human-readable name given to the key by the merchant.
    pub name: String,
    /// Public prefix of the raw key (first 12 characters).
    pub key_prefix: String,
    /// Hex SHA-256 hash of the raw key.
    pub key_hash: String,
    /// Permissions granted to this key.
    pub scopes: Vec<ApiScope>,
    /// When the key was last used successfully, if ever.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Whether the key has been revoked. Revoked keys must be rejected
    /// even if their hash matches.
    pub revoked: bool,
    /// When the key record was created.
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    /// Returns `true` if this key is currently usable for an operation
    /// requiring `required`: it is not revoked and at least one of its
    /// scopes allows the operation.
    pub fn allows(&self, required: ApiScope) -> bool {
        !self.revoked && self.scopes.iter().any(|scope| scope.allows(required))
    }

    /// Returns `true` if `raw` is the raw key this record was created
    /// from, by comparing hashes.
    pub fn matches_raw_key(&self, raw: &str) -> bool {
        self.key_hash == hash_key(raw)
    }
}

/// Hashes a raw API key for storage / comparison: the lowercase hex
/// encoding of its SHA-256 digest.
///
/// Deterministic: the same input always produces the same hash.
pub fn hash_key(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Returns the public prefix of a raw key: its first
/// [`DISPLAY_PREFIX_LEN`] (12) characters, or the whole string when
/// it is shorter than that.
pub fn prefix_of(raw: &str) -> String {
    raw.chars().take(DISPLAY_PREFIX_LEN).collect()
}

/// Monotonic per-process counter mixed into [`generate_key`] so two
/// keys generated within the same clock tick still differ.
static GENERATE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// One step of the SplitMix64 mixing function: cheap, deterministic,
/// and good enough to spread correlated seed inputs (timestamps,
/// counters) across the full `u64` range.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Generates a new raw API key.
///
/// Returns `(raw, prefix, hash)` where `raw` is the secret to show the
/// merchant once, `prefix` is [`prefix_of`] applied to it, and `hash`
/// is [`hash_key`] applied to it — exactly the values to persist on an
/// [`ApiKey`] record.
///
/// The raw key has the format `"mp_live_"` followed by 32 lowercase
/// hex characters.
///
/// # Entropy — not a CSPRNG
///
/// The hex material is derived using only the standard library, from
/// the current [`SystemTime`] in nanoseconds, an atomic per-process
/// counter, and the process id, mixed with SplitMix64. That makes keys
/// unpredictable to a casual observer and unique in practice, but it
/// is **not** a cryptographically secure random source: the seed space
/// is guessable by a determined attacker who knows roughly when a key
/// was created. Production deployments should swap this entropy
/// source for a real CSPRNG (e.g. the `rand` crate backed by the OS
/// RNG) while keeping the key format unchanged.
pub fn generate_key() -> (String, String, String) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let counter = GENERATE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = u64::from(std::process::id());

    // Seed the mixer from all three entropy inputs, decorrelated.
    let mut state = nanos
        ^ counter.wrapping_mul(0xD6E8_FEB8_6659_FD93)
        ^ pid.wrapping_mul(0xA076_1D64_78BD_642F);

    // 32 hex characters = 16 bytes = two u64 outputs.
    let hi = splitmix64(&mut state);
    let lo = splitmix64(&mut state);

    let raw = format!("{KEY_PREFIX}{hi:016x}{lo:016x}");
    debug_assert_eq!(raw.len(), KEY_PREFIX.len() + KEY_HEX_LEN);

    let prefix = prefix_of(&raw);
    let hash = hash_key(&raw);
    (raw, prefix, hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_key_is_deterministic() {
        let raw = "mp_live_0123456789abcdef0123456789abcdef";
        assert_eq!(hash_key(raw), hash_key(raw));
    }

    #[test]
    fn hash_key_differs_for_different_inputs() {
        assert_ne!(hash_key("mp_live_aaaa"), hash_key("mp_live_aaab"));
        assert_ne!(hash_key(""), hash_key("mp_live_aaaa"));
    }

    #[test]
    fn hash_key_matches_known_sha256_vectors() {
        // SHA-256("") and SHA-256("abc"), widely published test vectors.
        assert_eq!(
            hash_key(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hash_key("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hash_key_is_64_lowercase_hex_chars() {
        let hash = hash_key("anything");
        assert_eq!(hash.len(), 64);
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn prefix_of_takes_first_12_chars() {
        let raw = "mp_live_0123456789abcdef0123456789abcdef";
        assert_eq!(prefix_of(raw), "mp_live_0123");
        assert_eq!(prefix_of(raw).len(), 12);
    }

    #[test]
    fn prefix_of_short_input_returns_whole_input() {
        assert_eq!(prefix_of("mp_live_"), "mp_live_");
        assert_eq!(prefix_of(""), "");
        assert_eq!(prefix_of("short"), "short");
    }

    #[test]
    fn full_access_allows_every_scope() {
        for required in [
            ApiScope::PaymentsRead,
            ApiScope::PaymentsWrite,
            ApiScope::InvoicesWrite,
            ApiScope::WebhooksManage,
            ApiScope::RefundsWrite,
            ApiScope::FullAccess,
        ] {
            assert!(ApiScope::FullAccess.allows(required), "{required:?}");
        }
    }

    #[test]
    fn non_full_access_scope_allows_only_itself() {
        assert!(ApiScope::PaymentsRead.allows(ApiScope::PaymentsRead));
        assert!(!ApiScope::PaymentsRead.allows(ApiScope::PaymentsWrite));
        assert!(!ApiScope::PaymentsRead.allows(ApiScope::FullAccess));

        assert!(ApiScope::PaymentsWrite.allows(ApiScope::PaymentsWrite));
        assert!(!ApiScope::PaymentsWrite.allows(ApiScope::PaymentsRead));

        assert!(ApiScope::InvoicesWrite.allows(ApiScope::InvoicesWrite));
        assert!(!ApiScope::InvoicesWrite.allows(ApiScope::PaymentsWrite));

        assert!(ApiScope::WebhooksManage.allows(ApiScope::WebhooksManage));
        assert!(!ApiScope::WebhooksManage.allows(ApiScope::InvoicesWrite));

        assert!(ApiScope::RefundsWrite.allows(ApiScope::RefundsWrite));
        assert!(!ApiScope::RefundsWrite.allows(ApiScope::PaymentsWrite));
    }

    #[test]
    fn generate_key_has_expected_format() {
        let (raw, prefix, hash) = generate_key();
        assert!(raw.starts_with(KEY_PREFIX), "{raw}");
        assert_eq!(raw.len(), KEY_PREFIX.len() + KEY_HEX_LEN);
        assert!(raw[KEY_PREFIX.len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(prefix, prefix_of(&raw));
        assert_eq!(prefix.len(), 12);
        assert_eq!(hash, hash_key(&raw));
    }

    #[test]
    fn generate_key_produces_distinct_keys() {
        let (first, _, _) = generate_key();
        let (second, _, _) = generate_key();
        assert_ne!(first, second);
    }

    #[test]
    fn api_key_allows_checks_scopes_and_revocation() {
        let (raw, prefix, hash) = generate_key();
        let mut key = ApiKey {
            id: "key_1".to_string(),
            merchant_id: "mch_1".to_string(),
            name: "Production".to_string(),
            key_prefix: prefix,
            key_hash: hash,
            scopes: vec![ApiScope::PaymentsRead],
            last_used_at: None,
            revoked: false,
            created_at: Utc::now(),
        };
        assert!(key.allows(ApiScope::PaymentsRead));
        assert!(!key.allows(ApiScope::PaymentsWrite));
        assert!(key.matches_raw_key(&raw));
        assert!(!key.matches_raw_key("mp_live_wrong"));

        key.revoked = true;
        assert!(!key.allows(ApiScope::PaymentsRead));
    }
}
