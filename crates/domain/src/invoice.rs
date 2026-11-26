//! Invoices for MiraclePay.
//!
//! Original, clean-room implementation for MiraclePay v7.0.
//!
//! An [`Invoice`] is a bill issued by a merchant to a customer. It is
//! made up of one or more [`InvoiceItem`] line items, may carry a flat
//! discount, and may be taxed at a rate expressed in basis points
//! (`tax_bps`, where `10_000` bps = 100%).
//!
//! All amounts are handled in the currency's minor unit (for example
//! poisha for BDT or cents for USD) as `i64`, so no floating-point
//! rounding ever enters a total.
//!
//! Total calculation, in order:
//!
//! 1. `subtotal = sum(item.line_total())`
//! 2. `taxable  = max(subtotal - discount_minor, 0)`
//! 3. `tax      = round_half_up(taxable * tax_bps / 10_000)`
//! 4. `total    = taxable + tax`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::money::{Currency, Money};

/// Lifecycle state of an [`Invoice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// Created but not yet issued to the customer.
    Draft,
    /// Issued to the customer and awaiting payment.
    Sent,
    /// Fully paid; `paid_transaction_id` identifies the payment.
    Paid,
    /// Past its due date and still unpaid.
    Overdue,
    /// Cancelled by the merchant before payment.
    Cancelled,
    /// Voided: issued in error and no longer payable.
    Void,
}

impl InvoiceStatus {
    /// Whether this status is terminal (no further payment is expected).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            InvoiceStatus::Paid | InvoiceStatus::Cancelled | InvoiceStatus::Void
        )
    }
}

impl Default for InvoiceStatus {
    fn default() -> Self {
        InvoiceStatus::Draft
    }
}

/// A single line on an [`Invoice`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceItem {
    /// Human-readable description of what is being charged for.
    pub description: String,
    /// Number of units being charged for.
    pub quantity: u32,
    /// Price of one unit, in the invoice currency's minor unit.
    pub unit_amount_minor: i64,
}

impl InvoiceItem {
    /// Create a new line item.
    pub fn new(
        description: impl Into<String>,
        quantity: u32,
        unit_amount_minor: i64,
    ) -> Self {
        Self {
            description: description.into(),
            quantity,
            unit_amount_minor,
        }
    }

    /// Total for this line: `quantity * unit_amount_minor`.
    ///
    /// Saturates instead of overflowing on absurd inputs.
    pub fn line_total(&self) -> i64 {
        i64::from(self.quantity).saturating_mul(self.unit_amount_minor)
    }
}

/// A bill issued by a merchant to a customer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    /// Unique identifier of this invoice.
    pub id: String,
    /// Identifier of the merchant who issued the invoice.
    pub merchant_id: String,
    /// Merchant-facing invoice number, e.g. `INV-2026-0001`.
    pub number: String,
    /// Name of the customer being billed.
    pub customer_name: String,
    /// Email address of the customer being billed.
    pub customer_email: String,
    /// Line items making up the invoice.
    pub items: Vec<InvoiceItem>,
    /// Currency every amount on this invoice is denominated in.
    pub currency: Currency,
    /// Tax rate in basis points (`10_000` = 100%), applied to the
    /// discounted subtotal.
    pub tax_bps: u32,
    /// Flat discount, in minor units, subtracted from the subtotal
    /// before tax is calculated.
    pub discount_minor: i64,
    /// Current lifecycle state.
    pub status: InvoiceStatus,
    /// When payment is due, if a due date was set.
    pub due_at: Option<DateTime<Utc>>,
    /// Identifier of the transaction that paid this invoice, once paid.
    pub paid_transaction_id: Option<String>,
    /// When the invoice was created.
    pub created_at: DateTime<Utc>,
}

impl Invoice {
    /// Sum of every line item's total, in minor units, before
    /// discount and tax.
    pub fn subtotal_minor(&self) -> i64 {
        self.items
            .iter()
            .fold(0i64, |acc, item| acc.saturating_add(item.line_total()))
    }

    /// Subtotal after the flat discount, floored at zero, in minor units.
    /// This is the amount tax is calculated on.
    pub fn taxable_minor(&self) -> i64 {
        self.subtotal_minor()
            .saturating_sub(self.discount_minor)
            .max(0)
    }

    /// Tax owed, in minor units: `tax_bps` basis points of the
    /// discounted subtotal, rounded half-up to the nearest minor unit.
    pub fn tax_minor(&self) -> i64 {
        let taxable = self.taxable_minor();
        if taxable == 0 || self.tax_bps == 0 {
            return 0;
        }
        // taxable and tax_bps are both non-negative here, so adding
        // half the divisor before dividing rounds half-up.
        taxable
            .saturating_mul(i64::from(self.tax_bps))
            .saturating_add(5_000)
            / 10_000
    }

    /// Grand total payable: discounted subtotal plus tax, as [`Money`]
    /// in this invoice's currency.
    pub fn total(&self) -> Money {
        Money::new(
            self.taxable_minor().saturating_add(self.tax_minor()),
            self.currency,
        )
    }

    /// Grand total payable in minor units; see [`Invoice::total`].
    pub fn total_minor(&self) -> i64 {
        self.taxable_minor().saturating_add(self.tax_minor())
    }

    /// Whether this invoice is overdue at `now`.
    ///
    /// An invoice that has been paid, cancelled or voided is never
    /// overdue. Otherwise it is overdue when it is already flagged
    /// [`InvoiceStatus::Overdue`], or when its due date has passed.
    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        if self.status.is_terminal() {
            return false;
        }
        if self.status == InvoiceStatus::Overdue {
            return true;
        }
        match self.due_at {
            Some(due_at) => due_at < now,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(secs, 0).expect("valid timestamp")
    }

    fn invoice(items: Vec<InvoiceItem>, tax_bps: u32, discount_minor: i64) -> Invoice {
        Invoice {
            id: "inv_1".to_string(),
            merchant_id: "merch_1".to_string(),
            number: "INV-2026-0001".to_string(),
            customer_name: "Ayesha Rahman".to_string(),
            customer_email: "ayesha@example.com".to_string(),
            items,
            currency: Currency::BDT,
            tax_bps,
            discount_minor,
            status: InvoiceStatus::Sent,
            due_at: None,
            paid_transaction_id: None,
            created_at: at(1_000),
        }
    }

    #[test]
    fn line_total_multiplies_quantity_by_unit_amount() {
        let item = InvoiceItem::new("Hosting, monthly", 3, 1_500);
        assert_eq!(item.line_total(), 4_500);
    }

    #[test]
    fn line_total_is_zero_for_zero_quantity() {
        let item = InvoiceItem::new("Free add-on", 0, 9_999);
        assert_eq!(item.line_total(), 0);
    }

    #[test]
    fn subtotal_sums_all_line_totals() {
        let inv = invoice(
            vec![
                InvoiceItem::new("Domain", 1, 1_200),
                InvoiceItem::new("Hosting", 2, 2_500),
            ],
            0,
            0,
        );
        assert_eq!(inv.subtotal_minor(), 6_200);
    }

    #[test]
    fn empty_invoice_totals_zero() {
        let inv = invoice(vec![], 1_500, 0);
        assert_eq!(inv.subtotal_minor(), 0);
        assert_eq!(inv.tax_minor(), 0);
        assert_eq!(inv.total_minor(), 0);
        assert_eq!(inv.total(), Money::new(0, Currency::BDT));
    }

    #[test]
    fn total_without_tax_or_discount_is_subtotal() {
        let inv = invoice(vec![InvoiceItem::new("Service", 2, 5_000)], 0, 0);
        assert_eq!(inv.total_minor(), 10_000);
        assert_eq!(inv.total(), Money::new(10_000, Currency::BDT));
    }

    #[test]
    fn tax_is_applied_in_basis_points() {
        // 15% tax (1500 bps) on 10_000 minor = 1_500 minor.
        let inv = invoice(vec![InvoiceItem::new("Service", 1, 10_000)], 1_500, 0);
        assert_eq!(inv.tax_minor(), 1_500);
        assert_eq!(inv.total_minor(), 11_500);
    }

    #[test]
    fn discount_is_subtracted_before_tax() {
        // Subtotal 10_000, discount 2_000 -> taxable 8_000,
        // 10% tax (1000 bps) -> 800, total 8_800.
        let inv = invoice(vec![InvoiceItem::new("Service", 1, 10_000)], 1_000, 2_000);
        assert_eq!(inv.taxable_minor(), 8_000);
        assert_eq!(inv.tax_minor(), 800);
        assert_eq!(inv.total_minor(), 8_800);
        assert_eq!(inv.total(), Money::new(8_800, Currency::BDT));
    }

    #[test]
    fn discount_larger_than_subtotal_floors_total_at_zero() {
        let inv = invoice(vec![InvoiceItem::new("Service", 1, 1_000)], 1_500, 5_000);
        assert_eq!(inv.taxable_minor(), 0);
        assert_eq!(inv.tax_minor(), 0);
        assert_eq!(inv.total_minor(), 0);
    }

    #[test]
    fn tax_rounds_half_up_to_nearest_minor_unit() {
        // 5% (500 bps) of 10 minor = 0.5 minor -> rounds up to 1.
        let inv = invoice(vec![InvoiceItem::new("Fee", 1, 10)], 500, 0);
        assert_eq!(inv.tax_minor(), 1);
        assert_eq!(inv.total_minor(), 11);
    }

    #[test]
    fn total_carries_the_invoice_currency() {
        let mut inv = invoice(vec![InvoiceItem::new("Service", 1, 100)], 0, 0);
        inv.currency = Currency::USD;
        assert_eq!(inv.total(), Money::new(100, Currency::USD));
    }

    #[test]
    fn overdue_when_due_date_has_passed_and_unpaid() {
        let mut inv = invoice(vec![InvoiceItem::new("Service", 1, 100)], 0, 0);
        inv.due_at = Some(at(2_000));
        assert!(!inv.is_overdue(at(1_999)));
        assert!(!inv.is_overdue(at(2_000)));
        assert!(inv.is_overdue(at(2_001)));
    }

    #[test]
    fn not_overdue_without_a_due_date() {
        let inv = invoice(vec![InvoiceItem::new("Service", 1, 100)], 0, 0);
        assert!(!inv.is_overdue(at(9_999_999)));
    }

    #[test]
    fn paid_cancelled_and_void_are_never_overdue() {
        for status in [
            InvoiceStatus::Paid,
            InvoiceStatus::Cancelled,
            InvoiceStatus::Void,
        ] {
            let mut inv = invoice(vec![InvoiceItem::new("Service", 1, 100)], 0, 0);
            inv.status = status;
            inv.due_at = Some(at(1_000));
            assert!(!inv.is_overdue(at(9_999)), "status {status:?}");
        }
    }

    #[test]
    fn overdue_status_is_overdue_even_without_a_due_date() {
        let mut inv = invoice(vec![InvoiceItem::new("Service", 1, 100)], 0, 0);
        inv.status = InvoiceStatus::Overdue;
        assert!(inv.is_overdue(at(1_500)));
    }
}
