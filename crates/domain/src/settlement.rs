//! Merchant settlements for MiraclePay.
//!
//! A [`Settlement`] aggregates the payments collected for one merchant over
//! a period (for example a day or a week). The platform keeps a fee, and the
//! remainder — the *net* amount, [`Settlement::net`] — is paid out to the
//! merchant through a payout method such as a bank transfer or a mobile
//! wallet (bKash, Nagad, Rocket, Upay, and similar).
//!
//! All amounts are [`Money`] values in minor units, so no floating-point
//! rounding ever enters settlement arithmetic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::money::Money;

/// Lifecycle state of a [`Settlement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementStatus {
    /// Computed but not yet being paid out.
    Pending,
    /// The payout has been handed to the payout provider / bank.
    Processing,
    /// The merchant has been paid in full.
    Paid,
    /// The payout failed and needs attention before it can be retried.
    Failed,
    /// Held back, e.g. for a compliance or dispute review.
    OnHold,
}

impl SettlementStatus {
    /// Whether this is a final state that will not change on its own.
    ///
    /// `Paid` and `Failed` are terminal; `Pending`, `Processing`, and
    /// `OnHold` settlements can still move.
    pub fn is_terminal(self) -> bool {
        matches!(self, SettlementStatus::Paid | SettlementStatus::Failed)
    }
}

/// One merchant's settlement for a single period.
///
/// The net payout is deliberately *not* stored: it is always derived from
/// `gross - fee` by [`Settlement::net`], so the stored fields can never
/// disagree with the amount actually owed to the merchant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settlement {
    /// Unique settlement identifier.
    pub id: String,
    /// Merchant this settlement belongs to.
    pub merchant_id: String,
    /// Total amount collected for the merchant in the period, before fees.
    pub gross: Money,
    /// Platform fee deducted from `gross`.
    pub fee: Money,
    /// Current lifecycle state.
    pub status: SettlementStatus,
    /// Start of the period covered by this settlement (inclusive).
    pub period_start: DateTime<Utc>,
    /// End of the period covered by this settlement (exclusive).
    pub period_end: DateTime<Utc>,
    /// How the net amount is paid out, e.g. `"bank_transfer"` or `"bkash"`.
    pub payout_method: String,
    /// Reference issued by the payout provider once the payout is sent
    /// (bank transaction id, wallet transaction id, ...), if any yet.
    pub payout_reference: Option<String>,
    /// When this settlement record was created.
    pub created_at: DateTime<Utc>,
}

impl Settlement {
    /// Create a new settlement in [`SettlementStatus::Pending`] with no
    /// payout reference yet.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        merchant_id: impl Into<String>,
        gross: Money,
        fee: Money,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        payout_method: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            merchant_id: merchant_id.into(),
            gross,
            fee,
            status: SettlementStatus::Pending,
            period_start,
            period_end,
            payout_method: payout_method.into(),
            payout_reference: None,
            created_at: Utc::now(),
        }
    }

    /// Net amount owed to the merchant: `gross - fee`.
    ///
    /// Returns a [`DomainError`] when the two amounts cannot be subtracted:
    /// [`DomainError::CurrencyMismatch`] if `gross` and `fee` are in
    /// different currencies, or [`DomainError::Overflow`] if the result
    /// does not fit in an `i64` count of minor units.
    pub fn net(&self) -> Result<Money, DomainError> {
        self.gross.sub(self.fee)
    }
}

/// Compute a platform fee in minor units from a gross amount and a fee rate
/// expressed in basis points (1 bp = 0.01%, so 250 bps = 2.5%).
///
/// The fee is `gross_minor * fee_bps / 10_000`, rounded half-up to the
/// nearest minor unit. A non-positive gross amount has no fee. The
/// multiplication is done in `i128` and the result saturates at
/// `i64::MAX`, so no input can overflow.
///
/// For example, `compute_fee(100_000, 250)` is `2_500`: a 2.5% fee on
/// 1000.00 (100_000 minor units) is 25.00 (2_500 minor units).
pub fn compute_fee(gross_minor: i64, fee_bps: u32) -> i64 {
    if gross_minor <= 0 || fee_bps == 0 {
        return 0;
    }
    // +5_000 before dividing implements round-half-up for positive values.
    let fee = (i128::from(gross_minor) * i128::from(fee_bps) + 5_000) / 10_000;
    fee.min(i128::from(i64::MAX)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    fn money(minor: i64) -> Money {
        Money::new(minor, Currency::BDT)
    }

    fn settlement(gross: i64, fee: i64) -> Settlement {
        let start = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        Settlement::new(
            "stl_1",
            "mch_1",
            money(gross),
            money(fee),
            start,
            start + chrono::Duration::days(1),
            "bkash",
        )
    }

    // ---- compute_fee ---------------------------------------------------

    #[test]
    fn fee_is_zero_for_zero_rate_and_zero_gross() {
        assert_eq!(compute_fee(100_000, 0), 0);
        assert_eq!(compute_fee(0, 250), 0);
    }

    #[test]
    fn fee_matches_basis_points() {
        // 2.5% of 1000.00 (100_000 minor) is 25.00 (2_500 minor).
        assert_eq!(compute_fee(100_000, 250), 2_500);
        // 1% of 10.00 is 0.10.
        assert_eq!(compute_fee(1_000, 100), 10);
        // 100% takes everything.
        assert_eq!(compute_fee(12_345, 10_000), 12_345);
    }

    #[test]
    fn fee_rounds_half_up() {
        // 1% of 9.99 = 0.0999 -> 0.10 (10 minor).
        assert_eq!(compute_fee(999, 100), 10);
        // 1% of 9.49 = 0.0949 -> 0.09 (9 minor).
        assert_eq!(compute_fee(949, 100), 9);
        // Exactly half a minor unit rounds up: 0.5% of 1.00 = 0.005 -> 0.01.
        assert_eq!(compute_fee(100, 50), 1);
    }

    #[test]
    fn fee_is_zero_for_negative_gross() {
        assert_eq!(compute_fee(-50_000, 250), 0);
    }

    #[test]
    fn fee_does_not_overflow_on_huge_inputs() {
        assert_eq!(compute_fee(i64::MAX, 10_000), i64::MAX);
        assert_eq!(compute_fee(i64::MAX, u32::MAX), i64::MAX);
    }

    // ---- Settlement::net -----------------------------------------------

    #[test]
    fn net_is_gross_minus_fee() {
        let s = settlement(100_000, 2_500);
        assert_eq!(s.net().unwrap(), money(97_500));
    }

    #[test]
    fn net_is_zero_when_fee_equals_gross() {
        let s = settlement(5_000, 5_000);
        assert_eq!(s.net().unwrap(), money(0));
    }

    #[test]
    fn net_is_full_gross_when_no_fee() {
        let s = settlement(42_000, 0);
        assert_eq!(s.net().unwrap(), money(42_000));
    }

    #[test]
    fn net_propagates_error_from_money_subtraction() {
        // A fee in a different currency than the gross cannot be deducted.
        let start = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let s = Settlement::new(
            "stl_2",
            "mch_1",
            money(10_000),
            Money::new(100, Currency::USD),
            start,
            start + chrono::Duration::days(1),
            "bank_transfer",
        );
        assert!(s.net().is_err());
    }

    // ---- Settlement basics ----------------------------------------------

    #[test]
    fn new_settlement_starts_pending_without_payout_reference() {
        let s = settlement(1_000, 25);
        assert_eq!(s.status, SettlementStatus::Pending);
        assert_eq!(s.payout_reference, None);
        assert_eq!(s.id, "stl_1");
        assert_eq!(s.merchant_id, "mch_1");
        assert_eq!(s.payout_method, "bkash");
        assert!(s.period_end > s.period_start);
    }

    #[test]
    fn terminal_statuses_are_paid_and_failed() {
        assert!(SettlementStatus::Paid.is_terminal());
        assert!(SettlementStatus::Failed.is_terminal());
        assert!(!SettlementStatus::Pending.is_terminal());
        assert!(!SettlementStatus::Processing.is_terminal());
        assert!(!SettlementStatus::OnHold.is_terminal());
    }
}
