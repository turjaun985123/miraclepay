//! TOTP two-factor authentication (Wave 26) — RFC 6238 over
//! HMAC-SHA1 (RFC 4226 HOTP), the algorithm every authenticator
//! app (Google Authenticator, Authy, 1Password…) implements.
//!
//! Secrets are 20 random bytes, base32-encoded (RFC 4648, no
//! padding) for display/QR encoding. Verification accepts the
//! neighbouring time steps (±30 s) to tolerate clock skew, and the
//! login flow stores the secret server-side only — it is never
//! serialised into API responses after setup.

use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// Time-step length in seconds (the authenticator-app standard).
pub const PERIOD_SECS: u64 = 30;
/// Code length in digits (the authenticator-app standard).
pub const DIGITS: u32 = 6;

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Base32-encode (RFC 4648, uppercase, no padding).
pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in data {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32_ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE32_ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

/// Base32-decode, tolerating spaces and lowercase (users paste
/// secrets in many shapes). Returns `None` on invalid characters.
pub fn base32_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for ch in input.chars() {
        if ch == ' ' || ch == '-' {
            continue;
        }
        let ch = ch.to_ascii_uppercase();
        let value = BASE32_ALPHABET.iter().position(|&c| c as char == ch)? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// Generate a fresh base32 TOTP secret (20 CSPRNG bytes).
pub fn generate_secret() -> String {
    let mut bytes = [0u8; 20];
    OsRng.fill_bytes(&mut bytes);
    base32_encode(&bytes)
}

/// HOTP (RFC 4226) for `counter` with `secret` bytes.
pub fn hotp(secret: &[u8], counter: u64, digits: u32) -> String {
    let mut mac = HmacSha1::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    // Dynamic truncation (RFC 4226 §5.3).
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let binary = (u32::from(digest[offset]) & 0x7f) << 24
        | (u32::from(digest[offset + 1]) & 0xff) << 16
        | (u32::from(digest[offset + 2]) & 0xff) << 8
        | (u32::from(digest[offset + 3]) & 0xff);
    let code = binary % 10u32.pow(digits);
    format!("{code:0width$}", width = digits as usize)
}

/// TOTP code for a Unix timestamp.
pub fn totp_at(secret: &[u8], timestamp_secs: u64) -> String {
    hotp(secret, timestamp_secs / PERIOD_SECS, DIGITS)
}

/// Verify a user-supplied `code` against a base32 `secret` at
/// `timestamp_secs`, accepting ±1 step of clock skew. Codes must be
/// exactly [`DIGITS`] numeric characters.
pub fn verify(secret_b32: &str, code: &str, timestamp_secs: u64) -> bool {
    let code = code.trim();
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let Some(secret) = base32_decode(secret_b32) else {
        return false;
    };
    let step = timestamp_secs / PERIOD_SECS;
    [step.wrapping_sub(1), step, step + 1]
        .iter()
        .any(|&counter| hotp(&secret, counter, DIGITS) == code)
}

/// The `otpauth://` URI authenticator apps scan from a QR code.
pub fn otpauth_uri(secret_b32: &str, account: &str, issuer: &str) -> String {
    format!(
        "otpauth://totp/{issuer}:{account}?secret={secret_b32}&issuer={issuer}&digits={DIGITS}&period={PERIOD_SECS}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 Appendix B test secret (ASCII seed for SHA-1).
    const RFC_SECRET_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn base32_round_trips() {
        let data = b"12345678901234567890";
        assert_eq!(base32_encode(data), RFC_SECRET_B32);
        assert_eq!(base32_decode(RFC_SECRET_B32).unwrap(), data.to_vec());
        // Tolerant decoding: spaces + lowercase.
        assert_eq!(
            base32_decode("gezd gnbv gy3t qojq").unwrap(),
            base32_decode("GEZDGNBVGY3TQOJQ").unwrap()
        );
        assert!(base32_decode("not!base32").is_none());
    }

    #[test]
    fn rfc6238_vectors_sha1() {
        // RFC 6238 App. B (8-digit values truncated to 6 digits).
        let secret = base32_decode(RFC_SECRET_B32).unwrap();
        assert_eq!(totp_at(&secret, 59), "287082");
        assert_eq!(totp_at(&secret, 1_111_111_109), "081804");
        assert_eq!(totp_at(&secret, 1_234_567_890), "005924");
    }

    #[test]
    fn verify_accepts_neighbouring_steps_only() {
        let secret = generate_secret();
        let bytes = base32_decode(&secret).unwrap();
        let now = 1_700_000_000u64;
        let code_now = totp_at(&bytes, now);
        assert!(verify(&secret, &code_now, now));
        assert!(verify(&secret, &code_now, now + PERIOD_SECS)); // +1 step skew
        assert!(!verify(&secret, &code_now, now + 3 * PERIOD_SECS));
        assert!(!verify(&secret, "abcdef", now));
        assert!(!verify(&secret, "12345", now));
    }

    #[test]
    fn generated_secrets_are_unique_and_decodable() {
        let a = generate_secret();
        let b = generate_secret();
        assert_ne!(a, b);
        assert_eq!(base32_decode(&a).unwrap().len(), 20);
    }
}
