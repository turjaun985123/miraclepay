//! Merchant domain types for MiraclePay.
//!
//! Original clean-room implementation. A merchant is a business that
//! accepts payments through the platform, owned by a single user account.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lifecycle status of a merchant account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MerchantStatus {
    /// Created but not yet approved / activated.
    Pending,
    /// Approved and able to accept payments.
    Active,
    /// Temporarily disabled; cannot accept new payments.
    Suspended,
    /// Permanently closed.
    Closed,
}

impl Default for MerchantStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// A merchant (business) registered on the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merchant {
    /// Unique merchant identifier.
    pub id: String,
    /// Identifier of the user who owns this merchant.
    pub owner_user_id: String,
    /// Legal / trading name of the business.
    pub business_name: String,
    /// Contact email for the business.
    pub email: String,
    /// Optional contact phone number.
    pub phone: Option<String>,
    /// ISO 3166-1 alpha-2 country code, e.g. `"BD"`.
    pub country: String,
    /// Current lifecycle status.
    pub status: MerchantStatus,
    /// Optional URL notified about payment events for this merchant.
    pub webhook_url: Option<String>,
    /// When the merchant record was created.
    pub created_at: DateTime<Utc>,
}

/// Input required to create a new [`Merchant`].
///
/// The `id`, `status`, and `created_at` fields of [`Merchant`] are assigned
/// by the caller / persistence layer, not supplied here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateMerchant {
    /// Identifier of the user who will own the merchant.
    /// Optional on the wire: the API overwrites it from the caller's JWT.
    #[serde(default)]
    pub owner_user_id: String,
    /// Legal / trading name of the business.
    pub business_name: String,
    /// Contact email for the business.
    pub email: String,
    /// Optional contact phone number.
    pub phone: Option<String>,
    /// ISO 3166-1 alpha-2 country code, e.g. `"BD"`.
    pub country: String,
    /// Optional URL notified about payment events for this merchant.
    pub webhook_url: Option<String>,
}

/// Validate a [`CreateMerchant`] input.
///
/// Returns `Ok(())` when the input is acceptable, or `Err` containing
/// *all* validation problems found (one human-readable message per
/// problem), so callers can surface every issue at once.
///
/// Rules:
/// - `owner_user_id` must be non-empty.
/// - `business_name` must be non-empty (after trimming whitespace).
/// - `email` must be non-empty and contain an `@` with non-empty
///   local and domain parts.
/// - `country` must be exactly two ASCII alphabetic characters
///   (an ISO 3166-1 alpha-2 code, e.g. `"BD"`).
/// - `webhook_url`, when present and non-empty, must start with
///   `http://` or `https://`.
pub fn validate_create(input: &CreateMerchant) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    if input.owner_user_id.trim().is_empty() {
        errors.push("owner_user_id must not be empty".to_string());
    }

    if input.business_name.trim().is_empty() {
        errors.push("business_name must not be empty".to_string());
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

    let country = input.country.trim();
    if country.len() != 2 || !country.chars().all(|c| c.is_ascii_alphabetic()) {
        errors.push("country must be a 2-letter code".to_string());
    }

    if let Some(url) = input.webhook_url.as_deref().map(str::trim) {
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
            errors.push("webhook_url must start with http:// or https://".to_string());
        }
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

    fn valid_input() -> CreateMerchant {
        CreateMerchant {
            owner_user_id: "user_1".to_string(),
            business_name: "Acme Store".to_string(),
            email: "owner@acme.example".to_string(),
            phone: None,
            country: "BD".to_string(),
            webhook_url: None,
        }
    }

    #[test]
    fn valid_input_passes() {
        assert_eq!(validate_create(&valid_input()), Ok(()));
    }

    #[test]
    fn valid_input_with_phone_and_webhook_passes() {
        let mut input = valid_input();
        input.phone = Some("+8801700000000".to_string());
        input.webhook_url = Some("https://acme.example/webhooks/miraclepay".to_string());
        assert_eq!(validate_create(&input), Ok(()));
    }

    #[test]
    fn email_without_at_is_rejected() {
        let mut input = valid_input();
        input.email = "owner-acme.example".to_string();
        let errors = validate_create(&input).expect_err("must fail");
        assert!(errors.iter().any(|e| e.contains("email")), "{errors:?}");
    }

    #[test]
    fn email_with_empty_local_or_domain_is_rejected() {
        for bad in ["@acme.example", "owner@", "@", ""] {
            let mut input = valid_input();
            input.email = bad.to_string();
            assert!(validate_create(&input).is_err(), "email {bad:?} must fail");
        }
    }

    #[test]
    fn empty_business_name_is_rejected() {
        for bad in ["", "   "] {
            let mut input = valid_input();
            input.business_name = bad.to_string();
            let errors = validate_create(&input).expect_err("must fail");
            assert!(
                errors.iter().any(|e| e.contains("business_name")),
                "{errors:?}"
            );
        }
    }

    #[test]
    fn empty_owner_user_id_is_rejected() {
        let mut input = valid_input();
        input.owner_user_id = String::new();
        let errors = validate_create(&input).expect_err("must fail");
        assert!(
            errors.iter().any(|e| e.contains("owner_user_id")),
            "{errors:?}"
        );
    }

    #[test]
    fn country_must_be_two_letters() {
        for bad in ["", "B", "BGD", "B1", "12", "b d"] {
            let mut input = valid_input();
            input.country = bad.to_string();
            let errors = validate_create(&input).expect_err("must fail");
            assert!(
                errors.iter().any(|e| e.contains("country")),
                "country {bad:?}: {errors:?}"
            );
        }
        for good in ["BD", "bd", "IN", "US"] {
            let mut input = valid_input();
            input.country = good.to_string();
            assert_eq!(validate_create(&input), Ok(()), "country {good:?}");
        }
    }

    #[test]
    fn invalid_webhook_url_is_rejected() {
        let mut input = valid_input();
        input.webhook_url = Some("ftp://acme.example/hook".to_string());
        let errors = validate_create(&input).expect_err("must fail");
        assert!(
            errors.iter().any(|e| e.contains("webhook_url")),
            "{errors:?}"
        );
    }

    #[test]
    fn multiple_problems_are_all_reported() {
        let input = CreateMerchant {
            owner_user_id: String::new(),
            business_name: String::new(),
            email: "not-an-email".to_string(),
            phone: None,
            country: "BGD".to_string(),
            webhook_url: None,
        };
        let errors = validate_create(&input).expect_err("must fail");
        assert_eq!(errors.len(), 4, "{errors:?}");
    }

    #[test]
    fn merchant_status_defaults_to_pending() {
        assert_eq!(MerchantStatus::default(), MerchantStatus::Pending);
    }

    #[test]
    fn merchant_constructs_with_all_fields() {
        let merchant = Merchant {
            id: "mch_1".to_string(),
            owner_user_id: "user_1".to_string(),
            business_name: "Acme Store".to_string(),
            email: "owner@acme.example".to_string(),
            phone: None,
            country: "BD".to_string(),
            status: MerchantStatus::Active,
            webhook_url: None,
            created_at: Utc::now(),
        };
        assert_eq!(merchant.id, "mch_1");
        assert_eq!(merchant.status, MerchantStatus::Active);
        assert_eq!(merchant.clone(), merchant);
    }
}
