//! Password hashing for MiraclePay (Wave 3).
//!
//! Replaces the v0.0.1 placeholder (a plain SHA-256 of
//! `email::password`) with **Argon2id** — the OWASP-recommended
//! password hashing function — with a fresh CSPRNG salt per password.
//! Hashes are stored in PHC string format (`$argon2id$v=19$...`),
//! which embeds the algorithm, parameters, and salt, so parameters
//! can be raised later without invalidating existing hashes.
//!
//! Note: hashes created by the old placeholder scheme do not verify
//! under Argon2id. v0.0.1 keeps state in memory by default, so no
//! stored production hashes exist yet; on the Postgres path a
//! password reset would be required for any pre-Wave-3 account.

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;

/// Hash a plaintext password with Argon2id and a fresh random salt.
///
/// Each call returns a different PHC string for the same password
/// (random salt); use [`verify_password`] to check a candidate.
pub fn hash_password(plain: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .expect("Argon2id hashing with a generated salt does not fail")
        .to_string()
}

/// Verify a plaintext candidate against a stored PHC hash.
///
/// Returns `false` for a wrong password *and* for a malformed or
/// non-Argon2id stored value (e.g. a legacy placeholder hash), so
/// callers can treat every failure uniformly as "unauthorized".
pub fn verify_password(plain: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_password_verifies() {
        let hash = hash_password("correct horse battery staple");
        assert!(verify_password("correct horse battery staple", &hash));
    }

    #[test]
    fn wrong_password_is_rejected() {
        let hash = hash_password("password1");
        assert!(!verify_password("password2", &hash));
    }

    #[test]
    fn same_password_hashes_differ_due_to_random_salt() {
        let a = hash_password("same-password");
        let b = hash_password("same-password");
        assert_ne!(a, b);
        assert!(verify_password("same-password", &a));
        assert!(verify_password("same-password", &b));
    }

    #[test]
    fn hash_is_argon2id_phc_format() {
        let hash = hash_password("whatever1");
        assert!(hash.starts_with("$argon2id$"), "got: {hash}");
    }

    #[test]
    fn legacy_placeholder_hash_does_not_verify() {
        // SHA-256 hex from the pre-Wave-3 scheme must never verify.
        let legacy = crate::api_key::hash_key("a@b.c::password1");
        assert!(!verify_password("password1", &legacy));
    }

    #[test]
    fn garbage_hash_does_not_verify_or_panic() {
        assert!(!verify_password("x", "not-a-hash"));
        assert!(!verify_password("x", ""));
    }
}
