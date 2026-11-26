//! User domain types for MiraclePay.
//!
//! Original clean-room implementation. A user is a person (or system
//! account holder) who can sign in to the platform; merchants, staff,
//! administrators, and customers are all users distinguished by role.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Role of a [`User`], determining what the account may access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    /// Platform administrator with full access, including the admin panel.
    Admin,
    /// Business owner who accepts payments through the platform.
    Merchant,
    /// Member of a merchant's (or the platform's) staff.
    Staff,
    /// End customer who pays merchants.
    Customer,
}

impl UserRole {
    /// Whether this role is the platform administrator role.
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }

    /// Whether this role may access the admin panel.
    ///
    /// Only [`UserRole::Admin`] may; merchants, staff, and customers
    /// are all denied admin-panel access.
    pub fn can_access_admin_panel(&self) -> bool {
        matches!(self, Self::Admin)
    }
}

impl Default for UserRole {
    fn default() -> Self {
        Self::Customer
    }
}

/// A user account on the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Unique user identifier.
    pub id: String,
    /// Display / full name of the user.
    pub name: String,
    /// Login / contact email address.
    pub email: String,
    /// Hash of the user's password (never the plaintext password).
    ///
    /// Skipped during serialization so the hash is never exposed in
    /// API responses or other serialized output. It is still
    /// deserializable, so persistence layers can load stored records.
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// Role determining what this account may access.
    pub role: UserRole,
    /// Whether the user has verified ownership of `email`.
    pub email_verified: bool,
    /// When the user record was created.
    pub created_at: DateTime<Utc>,
}

/// Input required to register a new [`User`].
///
/// The `id`, `password_hash`, `role`, `email_verified`, and
/// `created_at` fields of [`User`] are derived or assigned by the
/// caller / persistence layer, not supplied here: `password` is the
/// plaintext submitted at registration and must be hashed by the
/// caller before it is stored as `password_hash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterInput {
    /// Display / full name of the new user.
    pub name: String,
    /// Login / contact email address for the new account.
    pub email: String,
    /// Plaintext password chosen at registration (to be hashed by
    /// the caller, never stored as-is).
    pub password: String,
}

/// Validate a [`RegisterInput`].
///
/// Returns `Ok(())` when the input is acceptable, or `Err` containing
/// *all* validation problems found (one human-readable message per
/// problem), so callers can surface every issue at once.
///
/// Rules:
/// - `name` must be non-empty (after trimming whitespace).
/// - `email` must contain an `@` with non-empty local and domain
///   parts.
/// - `password` must be at least 8 characters long and contain at
///   least one digit.
pub fn validate_register(input: &RegisterInput) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    if input.name.trim().is_empty() {
        errors.push("name must not be empty".to_string());
    }

    let email = input.email.trim();
    if email.is_empty() {
        errors.push("email must not be empty".to_string());
    } else {
        match email.split_once('@') {
            Some((local, domain)) if !local.is_empty() && !domain.is_empty() => {}
            _ => errors.push("email must contain '@' with text on both sides".to_string()),
        }
    }

    if input.password.chars().count() < 8 {
        errors.push("password must be at least 8 characters long".to_string());
    }
    if !input.password.chars().any(|c| c.is_ascii_digit()) {
        errors.push("password must contain at least one digit".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> RegisterInput {
        RegisterInput {
            name: "Ayesha Rahman".to_string(),
            email: "ayesha@example.com".to_string(),
            password: "secret42".to_string(),
        }
    }

    fn sample_user() -> User {
        User {
            id: "usr_1".to_string(),
            name: "Ayesha Rahman".to_string(),
            email: "ayesha@example.com".to_string(),
            password_hash: "$argon2id$stored-hash-value".to_string(),
            role: UserRole::Merchant,
            email_verified: true,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn valid_input_passes() {
        assert_eq!(validate_register(&valid_input()), Ok(()));
    }

    #[test]
    fn empty_name_is_rejected() {
        for bad in ["", "   "] {
            let mut input = valid_input();
            input.name = bad.to_string();
            let errors = validate_register(&input).expect_err("must fail");
            assert!(errors.iter().any(|e| e.contains("name")), "{errors:?}");
        }
    }

    #[test]
    fn email_without_at_is_rejected() {
        let mut input = valid_input();
        input.email = "ayesha-example.com".to_string();
        let errors = validate_register(&input).expect_err("must fail");
        assert!(errors.iter().any(|e| e.contains("email")), "{errors:?}");
    }

    #[test]
    fn email_with_empty_local_or_domain_is_rejected() {
        for bad in ["@example.com", "ayesha@", "@", ""] {
            let mut input = valid_input();
            input.email = bad.to_string();
            assert!(validate_register(&input).is_err(), "email {bad:?} must fail");
        }
    }

    #[test]
    fn short_password_is_rejected() {
        let mut input = valid_input();
        input.password = "abc123".to_string();
        let errors = validate_register(&input).expect_err("must fail");
        assert!(
            errors.iter().any(|e| e.contains("password") && e.contains("8")),
            "{errors:?}"
        );
    }

    #[test]
    fn password_without_digit_is_rejected() {
        let mut input = valid_input();
        input.password = "onlyletters".to_string();
        let errors = validate_register(&input).expect_err("must fail");
        assert!(
            errors.iter().any(|e| e.contains("digit")),
            "{errors:?}"
        );
    }

    #[test]
    fn password_of_exactly_eight_chars_with_digit_passes() {
        let mut input = valid_input();
        input.password = "abcd1234".to_string();
        assert_eq!(validate_register(&input), Ok(()));
    }

    #[test]
    fn multiple_problems_are_all_reported() {
        let input = RegisterInput {
            name: String::new(),
            email: "not-an-email".to_string(),
            password: "short".to_string(),
        };
        let errors = validate_register(&input).expect_err("must fail");
        // name, email, password length, and password digit.
        assert_eq!(errors.len(), 4, "{errors:?}");
    }

    #[test]
    fn only_admin_is_admin() {
        assert!(UserRole::Admin.is_admin());
        for role in [UserRole::Merchant, UserRole::Staff, UserRole::Customer] {
            assert!(!role.is_admin(), "{role:?} must not be admin");
        }
    }

    #[test]
    fn only_admin_can_access_admin_panel() {
        assert!(UserRole::Admin.can_access_admin_panel());
        for role in [UserRole::Merchant, UserRole::Staff, UserRole::Customer] {
            assert!(
                !role.can_access_admin_panel(),
                "{role:?} must not access the admin panel"
            );
        }
    }

    #[test]
    fn user_role_defaults_to_customer() {
        assert_eq!(UserRole::default(), UserRole::Customer);
    }

    #[test]
    fn password_hash_never_serializes() {
        let user = sample_user();
        let json = serde_json::to_string(&user).expect("user serializes");
        assert!(
            !json.contains("password_hash"),
            "serialized user must not name the field: {json}"
        );
        assert!(
            !json.contains(&user.password_hash),
            "serialized user must not leak the hash value: {json}"
        );
        // The rest of the record still serializes normally.
        assert!(json.contains("\"ayesha@example.com\""), "{json}");
        assert!(json.contains("\"merchant\""), "{json}");
    }

    #[test]
    fn user_round_trips_through_deserialization() {
        // Deserialization still accepts a stored password_hash, so
        // persistence layers can load records; only serialization
        // withholds it.
        let stored = r#"{
            "id": "usr_1",
            "name": "Ayesha Rahman",
            "email": "ayesha@example.com",
            "password_hash": "$argon2id$stored-hash-value",
            "role": "merchant",
            "email_verified": true,
            "created_at": "2026-11-26T00:00:00Z"
        }"#;
        let user: User = serde_json::from_str(stored).expect("stored user parses");
        assert_eq!(user, sample_user_without_time());
    }

    fn sample_user_without_time() -> User {
        let mut user = sample_user();
        user.created_at =
            DateTime::parse_from_rfc3339("2026-11-26T00:00:00Z")
                .expect("timestamp parses")
                .with_timezone(&Utc);
        user
    }

    #[test]
    fn user_constructs_with_all_fields() {
        let user = sample_user();
        assert_eq!(user.id, "usr_1");
        assert_eq!(user.role, UserRole::Merchant);
        assert!(user.email_verified);
        assert_eq!(user.clone(), user);
    }
}
