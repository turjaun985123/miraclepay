//! Refunds against a previously settled transaction.
//!
//! Clean-room domain type for MiraclePay v7.0.
//!
//! A refund returns money for part (or all) of an earlier transaction
//! back to the original payer, through the same gateway that processed
//! the transaction. A refund never creates money: the sum of every
//! refund issued against a transaction must stay within that
//! transaction's amount. That invariant is enforced, before any
//! gateway call is made, by [`validate_refund`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::money::Money;

/// Lifecycle of a single refund.
///
/// A refund starts as [`RefundStatus::Pending`], moves to
/// [`RefundStatus::Processing`] once it has been handed to the
/// gateway, and ends in exactly one terminal state:
/// [`RefundStatus::Succeeded`] or [`RefundStatus::Failed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundStatus {
    /// Created locally, not yet submitted to the gateway.
    Pending,
    /// Submitted to the gateway, awaiting its final verdict.
    Processing,
    /// The gateway confirmed the money was returned. Terminal.
    Succeeded,
    /// The gateway rejected the refund, or it could not be completed.
    /// Terminal.
    Failed,
}

impl RefundStatus {
    /// Whether this is a terminal state: no further transitions are
    /// expected once a refund has succeeded or failed.
    pub fn is_terminal(self) -> bool {
        matches!(self, RefundStatus::Succeeded | RefundStatus::Failed)
    }

    /// Whether the refund is still in flight (`Pending` or
    /// `Processing`) and may yet change state.
    pub fn is_in_flight(self) -> bool {
        !self.is_terminal()
    }
}

impl std::fmt::Display for RefundStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            RefundStatus::Pending => "pending",
            RefundStatus::Processing => "processing",
            RefundStatus::Succeeded => "succeeded",
            RefundStatus::Failed => "failed",
        };
        f.write_str(label)
    }
}

/// A refund of (part of) one transaction, issued by one merchant.
///
/// `amount` is always positive and, together with every other refund
/// against the same transaction, never exceeds the transaction's
/// amount — see [`validate_refund`]. Amounts are carried as
/// [`Money`], so the currency is the transaction's currency and all
/// arithmetic upstream is done in integer minor units.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Refund {
    /// Unique identifier of this refund (assigned by MiraclePay).
    pub id: String,
    /// Identifier of the transaction being refunded.
    pub transaction_id: String,
    /// Identifier of the merchant who owns the transaction.
    pub merchant_id: String,
    /// Amount to return to the payer, in the transaction's currency.
    pub amount: Money,
    /// Optional free-text reason supplied by the merchant or support.
    pub reason: Option<String>,
    /// Current lifecycle state of the refund.
    pub status: RefundStatus,
    /// The gateway's own identifier for this refund, once the gateway
    /// has accepted it. `None` while the refund is purely local.
    pub provider_refund_id: Option<String>,
    /// When this refund record was created (UTC).
    pub created_at: DateTime<Utc>,
}

impl Refund {
    /// Create a new refund in [`RefundStatus::Pending`], timestamped
    /// now, with no gateway reference yet.
    ///
    /// This constructor does not check `amount` against the
    /// transaction: callers must run [`validate_refund`] with the
    /// transaction's amount and the total already refunded before
    /// persisting or submitting a refund.
    pub fn new(
        id: String,
        transaction_id: String,
        merchant_id: String,
        amount: Money,
        reason: Option<String>,
    ) -> Self {
        Self {
            id,
            transaction_id,
            merchant_id,
            amount,
            reason,
            status: RefundStatus::Pending,
            provider_refund_id: None,
            created_at: Utc::now(),
        }
    }

    /// Whether this refund has reached a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Whether this refund is still in flight and may change state.
    pub fn is_in_flight(&self) -> bool {
        self.status.is_in_flight()
    }
}

/// Validate a prospective refund against its transaction.
///
/// All amounts are integer minor units (for example poisha or cents)
/// of the transaction's currency:
///
/// * `tx_amount_minor` — the full amount of the original transaction;
/// * `already_refunded_minor` — the total of all earlier refunds
///   against that transaction (in any non-failed state);
/// * `requested_minor` — the amount this new refund would return.
///
/// # Rules
///
/// * The requested amount must be strictly positive, the transaction
///   amount must be strictly positive, and the already-refunded total
///   must not be negative — otherwise
///   [`DomainError::InvalidAmount`].
/// * `already_refunded_minor + requested_minor` must not exceed
///   `tx_amount_minor` — otherwise
///   [`DomainError::RefundExceedsTransaction`]. A refund that brings
///   the total to exactly the transaction amount (a full refund of
///   the remainder) is allowed.
/// * The addition is checked: an overflowing total is reported as
///   [`DomainError::RefundExceedsTransaction`] rather than wrapping.
///
/// # Errors
///
/// Returns [`DomainError::InvalidAmount`] or
/// [`DomainError::RefundExceedsTransaction`] as described above.
pub fn validate_refund(
    tx_amount_minor: i64,
    already_refunded_minor: i64,
    requested_minor: i64,
) -> Result<(), DomainError> {
    if requested_minor <= 0 {
        return Err(DomainError::InvalidAmount);
    }
    if tx_amount_minor <= 0 {
        return Err(DomainError::InvalidAmount);
    }
    if already_refunded_minor < 0 {
        return Err(DomainError::InvalidAmount);
    }

    // A ledger that already shows more refunded than was charged is
    // inconsistent; refuse to add anything further on top of it.
    if already_refunded_minor > tx_amount_minor {
        return Err(DomainError::RefundExceedsTransaction);
    }

    let total_refunded_minor = already_refunded_minor
        .checked_add(requested_minor)
        .ok_or(DomainError::RefundExceedsTransaction)?;

    if total_refunded_minor > tx_amount_minor {
        return Err(DomainError::RefundExceedsTransaction);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_refund: happy paths --------------------------------

    #[test]
    fn partial_refund_with_nothing_refunded_before_is_ok() {
        assert!(validate_refund(10_000, 0, 2_500).is_ok());
    }

    #[test]
    fn full_refund_of_the_whole_transaction_is_ok() {
        assert!(validate_refund(10_000, 0, 10_000).is_ok());
    }

    #[test]
    fn refunding_exactly_the_remaining_balance_is_ok() {
        // 4_000 already refunded of 10_000; the last 6_000 is allowed.
        assert!(validate_refund(10_000, 4_000, 6_000).is_ok());
    }

    #[test]
    fn small_partial_refund_after_partial_refund_is_ok() {
        assert!(validate_refund(10_000, 9_999, 1).is_ok());
    }

    #[test]
    fn one_minor_unit_refund_is_ok() {
        assert!(validate_refund(1, 0, 1).is_ok());
    }

    // --- validate_refund: amount must be positive --------------------

    #[test]
    fn zero_requested_amount_is_invalid() {
        assert!(matches!(
            validate_refund(10_000, 0, 0),
            Err(DomainError::InvalidAmount)
        ));
    }

    #[test]
    fn negative_requested_amount_is_invalid() {
        assert!(matches!(
            validate_refund(10_000, 0, -500),
            Err(DomainError::InvalidAmount)
        ));
    }

    #[test]
    fn zero_transaction_amount_is_invalid() {
        assert!(matches!(
            validate_refund(0, 0, 1),
            Err(DomainError::InvalidAmount)
        ));
    }

    #[test]
    fn negative_transaction_amount_is_invalid() {
        assert!(matches!(
            validate_refund(-10_000, 0, 1),
            Err(DomainError::InvalidAmount)
        ));
    }

    #[test]
    fn negative_already_refunded_total_is_invalid() {
        assert!(matches!(
            validate_refund(10_000, -1, 100),
            Err(DomainError::InvalidAmount)
        ));
    }

    // --- validate_refund: total must not exceed the transaction ------

    #[test]
    fn refund_larger_than_the_transaction_is_rejected() {
        assert!(matches!(
            validate_refund(10_000, 0, 10_001),
            Err(DomainError::RefundExceedsTransaction)
        ));
    }

    #[test]
    fn refund_one_unit_over_the_remaining_balance_is_rejected() {
        assert!(matches!(
            validate_refund(10_000, 4_000, 6_001),
            Err(DomainError::RefundExceedsTransaction)
        ));
    }

    #[test]
    fn any_further_refund_after_a_full_refund_is_rejected() {
        assert!(matches!(
            validate_refund(10_000, 10_000, 1),
            Err(DomainError::RefundExceedsTransaction)
        ));
    }

    #[test]
    fn already_refunded_above_transaction_amount_is_rejected() {
        assert!(matches!(
            validate_refund(10_000, 12_000, 1),
            Err(DomainError::RefundExceedsTransaction)
        ));
    }

    #[test]
    fn overflowing_total_is_rejected_instead_of_wrapping() {
        // already + requested would overflow i64; it must surface as
        // "exceeds the transaction", never wrap to a negative total
        // that slips past the comparison.
        assert!(matches!(
            validate_refund(i64::MAX, i64::MAX - 1, i64::MAX - 1),
            Err(DomainError::RefundExceedsTransaction)
        ));
    }

    #[test]
    fn invalid_amount_takes_precedence_over_exceeds_check() {
        // A non-positive request is InvalidAmount even when the
        // already-refunded total is also inconsistent.
        assert!(matches!(
            validate_refund(10_000, 12_000, 0),
            Err(DomainError::InvalidAmount)
        ));
    }

    // --- RefundStatus -------------------------------------------------

    #[test]
    fn succeeded_and_failed_are_terminal() {
        assert!(RefundStatus::Succeeded.is_terminal());
        assert!(RefundStatus::Failed.is_terminal());
        assert!(!RefundStatus::Succeeded.is_in_flight());
        assert!(!RefundStatus::Failed.is_in_flight());
    }

    #[test]
    fn pending_and_processing_are_in_flight() {
        assert!(RefundStatus::Pending.is_in_flight());
        assert!(RefundStatus::Processing.is_in_flight());
        assert!(!RefundStatus::Pending.is_terminal());
        assert!(!RefundStatus::Processing.is_terminal());
    }

    #[test]
    fn status_display_uses_snake_case_labels() {
        assert_eq!(RefundStatus::Pending.to_string(), "pending");
        assert_eq!(RefundStatus::Processing.to_string(), "processing");
        assert_eq!(RefundStatus::Succeeded.to_string(), "succeeded");
        assert_eq!(RefundStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn status_is_copy_and_compares_by_value() {
        let status = RefundStatus::Processing;
        let copied = status; // Copy: original remains usable.
        assert_eq!(status, copied);
        assert_ne!(status, RefundStatus::Pending);
    }

    #[test]
    fn status_debug_names_the_variant() {
        assert_eq!(format!("{:?}", RefundStatus::Pending), "Pending");
        assert_eq!(format!("{:?}", RefundStatus::Succeeded), "Succeeded");
    }
}
