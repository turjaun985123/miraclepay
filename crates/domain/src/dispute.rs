//! Disputes / chargebacks (Wave 38).
//!
//! A dispute is a payer's challenge against a completed payment,
//! raised through their bank or wallet provider. The platform
//! records the dispute, collects the merchant's evidence, and an
//! admin records the outcome.
//!
//! Money honesty: a dispute record by itself moves **no** money.
//! When a dispute is decided `Lost`, the platform emits
//! `dispute.closed` and the finance team performs the clawback
//! through the settlement flow; the record here is the audit
//! trail, not a transfer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::money::Money;
use crate::transaction::{Transaction, TxStatus};

/// Why the payer challenged the payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisputeReason {
    Fraudulent,
    ProductNotReceived,
    Duplicate,
    CreditNotProcessed,
    Other,
}

/// Lifecycle of a dispute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisputeStatus {
    Opened,
    UnderReview,
    Won,
    Lost,
    Closed,
}

impl DisputeStatus {
    /// Terminal states no longer accept evidence or decisions.
    pub fn is_terminal(self) -> bool {
        matches!(self, DisputeStatus::Won | DisputeStatus::Lost | DisputeStatus::Closed)
    }
}

/// One dispute record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dispute {
    pub id: String,
    pub merchant_id: String,
    pub transaction_id: String,
    /// Amount challenged (never more than the transaction amount).
    pub amount: Money,
    pub reason: DisputeReason,
    pub status: DisputeStatus,
    /// Merchant's evidence note (set via the evidence endpoint).
    pub evidence_note: Option<String>,
    /// Admin's closing note.
    pub outcome_note: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

/// Validate opening a dispute against `tx` for `amount`.
/// Collects every problem, mirroring the settings validators.
pub fn validate_open(tx: &Transaction, amount: Money) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if tx.status != TxStatus::Completed {
        errors.push(format!(
            "only a completed transaction can be disputed (status: {:?})",
            tx.status
        ));
    }
    if amount.currency != tx.amount.currency {
        errors.push("dispute currency must match the transaction currency".to_string());
    }
    if amount.amount_minor <= 0 {
        errors.push("dispute amount must be positive".to_string());
    }
    if amount.amount_minor > tx.amount.amount_minor {
        errors.push(format!(
            "dispute amount {} exceeds the transaction amount {}",
            amount.amount_minor, tx.amount.amount_minor
        ));
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
    use crate::money::Currency;

    fn make_tx(status: TxStatus) -> Transaction {
        let now = Utc::now();
        Transaction {
            id: "tx_1".to_string(),
            merchant_id: "mch_1".to_string(),
            gateway: "bkash".to_string(),
            amount: Money::new(50_000, Currency::BDT),
            status,
            customer_email: None,
            customer_phone: None,
            provider_trx_id: None,
            sender_number: None,
            reference: None,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn a_completed_payment_can_be_disputed_up_to_its_amount() {
        let tx = make_tx(TxStatus::Completed);
        assert_eq!(validate_open(&tx, Money::new(50_000, Currency::BDT)), Ok(()));
        assert_eq!(validate_open(&tx, Money::new(1, Currency::BDT)), Ok(()));
    }

    #[test]
    fn invalid_opens_collect_errors() {
        let tx = make_tx(TxStatus::Pending);
        let errors = validate_open(&tx, Money::new(60_000, Currency::USD)).unwrap_err();
        assert_eq!(errors.len(), 3, "{errors:?}"); // status + currency + exceeds
        let done = make_tx(TxStatus::Completed);
        assert!(validate_open(&done, Money::new(0, Currency::BDT)).is_err());
    }

    #[test]
    fn terminal_states_are_final() {
        assert!(DisputeStatus::Won.is_terminal());
        assert!(DisputeStatus::Lost.is_terminal());
        assert!(!DisputeStatus::Opened.is_terminal());
        assert!(!DisputeStatus::UnderReview.is_terminal());
    }
}
