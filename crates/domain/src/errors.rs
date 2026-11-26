//! Shared error type for the MiraclePay domain crate (v0.0.1).
//!
//! Every fallible domain operation returns [`DomainResult`], whose error
//! is [`DomainError`]. The variants are deliberately coarse-grained:
//! callers at the API boundary map them onto transport-level statuses
//! (for example `NotFound` -> 404, `Unauthorized` -> 401,
//! `Forbidden` -> 403, `Conflict` -> 409, and `Validation` /
//! `InvalidAmount` -> 422) without having to inspect free-form text.
//!
//! The parameterless variants are kept as unit variants on purpose:
//! sibling modules such as `money`, `payment_link`, and `refund`
//! construct and compare them directly (for example
//! `Err(DomainError::CurrencyMismatch)`), so they must not gain fields.

/// Errors that can arise from domain rules and invariants.
///
/// Variants carrying a `String` hold a human-readable detail message
/// supplied by the failing operation; unit variants are self-describing
/// through their [`std::fmt::Display`] message alone.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// Two monetary amounts in different currencies were combined.
    #[error("currency mismatch: amounts are in different currencies")]
    CurrencyMismatch,

    /// An arithmetic operation on a monetary amount overflowed.
    #[error("amount overflow: arithmetic result is out of range")]
    Overflow,

    /// A monetary amount was not valid (for example zero or negative
    /// where a strictly positive amount is required).
    #[error("invalid amount")]
    InvalidAmount,

    /// A refund would exceed the amount of its original transaction.
    #[error("refund exceeds the original transaction amount")]
    RefundExceedsTransaction,

    /// A payment link was used while it is inactive.
    #[error("payment link is inactive")]
    LinkInactive,

    /// A payment link has already reached its maximum number of uses.
    #[error("payment link has reached its maximum number of uses")]
    LinkExhausted,

    /// The requested entity does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// Input failed domain validation.
    #[error("validation failed: {0}")]
    Validation(String),

    /// The caller is not authenticated.
    #[error("unauthorized: authentication is required")]
    Unauthorized,

    /// The caller is authenticated but not allowed to perform the action.
    #[error("forbidden: the caller is not allowed to perform this action")]
    Forbidden,

    /// The operation conflicts with the current state of a resource.
    #[error("conflict: {0}")]
    Conflict(String),

    /// An entity was asked to make a state transition it does not allow.
    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),

    /// An unexpected internal failure occurred inside the domain layer.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Convenience alias for fallible domain operations.
pub type DomainResult<T> = Result<T, DomainError>;

#[cfg(test)]
mod tests {
    use super::DomainError;
    use std::collections::HashSet;

    /// One representative instance of every variant.
    fn all_variants() -> Vec<DomainError> {
        vec![
            DomainError::CurrencyMismatch,
            DomainError::Overflow,
            DomainError::InvalidAmount,
            DomainError::RefundExceedsTransaction,
            DomainError::LinkInactive,
            DomainError::LinkExhausted,
            DomainError::NotFound("invoice inv_1".to_string()),
            DomainError::Validation("email is malformed".to_string()),
            DomainError::Unauthorized,
            DomainError::Forbidden,
            DomainError::Conflict("invoice already paid".to_string()),
            DomainError::InvalidStateTransition("paid -> pending".to_string()),
            DomainError::Internal("unexpected invariant failure".to_string()),
        ]
    }

    #[test]
    fn display_messages_are_non_empty() {
        for error in all_variants() {
            let message = error.to_string();
            assert!(
                !message.trim().is_empty(),
                "Display message must not be empty for {error:?}"
            );
        }
    }

    #[test]
    fn display_messages_are_distinct() {
        // Distinct-ish: no two variants share the same Display text when
        // constructed with different payloads, so a logged message alone
        // identifies which variant produced it.
        let messages: Vec<String> = all_variants().iter().map(ToString::to_string).collect();
        let unique: HashSet<&String> = messages.iter().collect();
        assert_eq!(
            unique.len(),
            messages.len(),
            "Display messages must be distinct across variants: {messages:?}"
        );
    }

    #[test]
    fn payload_variants_include_their_detail() {
        assert!(DomainError::NotFound("merchant m_1".to_string())
            .to_string()
            .contains("merchant m_1"));
        assert!(DomainError::Validation("amount is required".to_string())
            .to_string()
            .contains("amount is required"));
        assert!(DomainError::Conflict("duplicate reference".to_string())
            .to_string()
            .contains("duplicate reference"));
        assert!(DomainError::InvalidStateTransition("refunded -> paid".to_string())
            .to_string()
            .contains("refunded -> paid"));
        assert!(DomainError::Internal("boom".to_string())
            .to_string()
            .contains("boom"));
    }

    #[test]
    fn unit_variants_compare_equal_to_themselves() {
        // Sibling modules assert on these variants by equality, so the
        // unit variants must remain directly comparable.
        assert_eq!(DomainError::CurrencyMismatch, DomainError::CurrencyMismatch);
        assert_eq!(DomainError::Overflow, DomainError::Overflow);
        assert_eq!(DomainError::InvalidAmount, DomainError::InvalidAmount);
        assert_eq!(
            DomainError::RefundExceedsTransaction,
            DomainError::RefundExceedsTransaction
        );
        assert_eq!(DomainError::LinkInactive, DomainError::LinkInactive);
        assert_eq!(DomainError::LinkExhausted, DomainError::LinkExhausted);
        assert_ne!(DomainError::LinkInactive, DomainError::LinkExhausted);
        assert_ne!(DomainError::Unauthorized, DomainError::Forbidden);
    }

    #[test]
    fn domain_result_alias_carries_domain_error() {
        fn fails() -> super::DomainResult<()> {
            Err(DomainError::InvalidAmount)
        }
        assert_eq!(fails(), Err(DomainError::InvalidAmount));

        fn succeeds() -> super::DomainResult<u32> {
            Ok(42)
        }
        assert_eq!(succeeds(), Ok(42));
    }

    #[test]
    fn error_implements_std_error() {
        fn assert_std_error<E: std::error::Error>(_: &E) {}
        for error in all_variants() {
            assert_std_error(&error);
        }
    }
}
