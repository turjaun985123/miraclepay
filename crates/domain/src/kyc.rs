//! Merchant KYC-lite (Wave 33).
//!
//! A lightweight know-your-customer profile for merchants: the
//! business declares its type, trade licence, national id (NID),
//! and address; an admin reviews and verifies or rejects it.
//!
//! Flow: `NotSubmitted → Pending → Verified | Rejected`
//! (resubmission after rejection returns to `Pending`).
//!
//! Enforcement in v0.0.1 is deliberately narrow and honest: a
//! **Rejected** merchant cannot be activated (enforced at the
//! admin status endpoint); merchants without a profile keep the
//! pre-KYC behaviour. Full gating (no activation without Verified)
//! is a policy decision for the operator, not silently imposed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Review state of a merchant's KYC profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KycStatus {
    Pending,
    Verified,
    Rejected,
}

/// Business types accepted at submission.
pub const BUSINESS_TYPES: &[&str] = &[
    "sole_proprietorship",
    "partnership",
    "limited",
    "online_seller",
    "freelancer",
    "other",
];

/// A merchant's submitted KYC profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KycProfile {
    pub merchant_id: String,
    pub business_type: String,
    /// Trade licence number (optional for freelancers/online sellers).
    pub trade_license_no: Option<String>,
    /// National ID number — validated by [`valid_nid`], stored as
    /// submitted. (Operators should treat this as sensitive PII.)
    pub nid_no: String,
    pub address: String,
    pub status: KycStatus,
    /// Admin's note on rejection (or verification), when given.
    pub decision_note: Option<String>,
    pub submitted_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

/// BD NID numbers are 10, 13, or 17 digits.
pub fn valid_nid(nid: &str) -> bool {
    let digits = nid.trim();
    digits.bytes().all(|b| b.is_ascii_digit())
        && matches!(digits.len(), 10 | 13 | 17)
}

/// Validate a submission; returns every problem found (the API
/// joins them into one 400 message).
pub fn validate_submission(
    business_type: &str,
    trade_license_no: Option<&str>,
    nid_no: &str,
    address: &str,
) -> Vec<String> {
    let mut errors = Vec::new();
    if !BUSINESS_TYPES.contains(&business_type) {
        errors.push(format!(
            "business_type must be one of: {}",
            BUSINESS_TYPES.join(", ")
        ));
    }
    if !valid_nid(nid_no) {
        errors.push("nid_no must be a 10, 13, or 17 digit national id number".to_string());
    }
    if address.trim().len() < 8 {
        errors.push("address is too short".to_string());
    }
    if let Some(license) = trade_license_no {
        let license = license.trim();
        if !license.is_empty()
            && (license.len() < 6 || !license.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'/'))
        {
            errors.push("trade_license_no looks malformed".to_string());
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nid_lengths() {
        assert!(valid_nid("1234567890")); // 10
        assert!(valid_nid("1234567890123")); // 13
        assert!(valid_nid("12345678901234567")); // 17
        assert!(!valid_nid("123456789")); // 9
        assert!(!valid_nid("12345678901")); // 11
        assert!(!valid_nid("12345abcde"));
        assert!(!valid_nid(""));
    }

    #[test]
    fn submission_validation_collects_errors() {
        let errors = validate_submission("wrong", None, "123", "short");
        assert_eq!(errors.len(), 3);
        let ok = validate_submission(
            "online_seller",
            Some("TRAD/DHAKA/2026-12345"),
            "1234567890",
            "House 1, Road 2, Dhaka 1212",
        );
        assert!(ok.is_empty(), "{ok:?}");
    }
}
