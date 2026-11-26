//! Payment transaction aggregate for MiraclePay.
//!
//! A [`Transaction`] records a single attempt by a customer to pay a
//! merchant through one payment gateway (for example a mobile wallet, a
//! card processor, or a bank transfer channel). The lifecycle of a
//! transaction is modelled by [`TxStatus`]; the allowed moves between
//! statuses are enforced by [`TxStatus::can_transition_to`], so callers
//! can validate a state change before applying it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::money::Money;

/// Lifecycle status of a [`Transaction`].
///
/// The state machine is intentionally small and one-directional:
///
/// ```text
/// Pending ──► Processing ──► Completed ──► Refunded
///    │            │              │
///    │            │              └──► PartiallyRefunded ──► Refunded
///    │            └──► Failed
///    ├──► Failed
///    └──► Expired
/// ```
///
/// `Failed`, `Refunded`, and `Expired` are final: once a transaction
/// reaches one of them it never changes status again (see
/// [`TxStatus::is_final`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxStatus {
    /// Created, but the gateway has not started processing it yet.
    Pending,
    /// The gateway is processing the payment (awaiting the customer,
    /// the provider, or a verification step such as an SMS match).
    Processing,
    /// The payment succeeded and the funds were captured.
    Completed,
    /// The payment attempt failed.
    Failed,
    /// A completed payment was fully refunded to the customer.
    Refunded,
    /// A completed payment was refunded in part; the remainder is
    /// still captured.
    PartiallyRefunded,
    /// The transaction was never completed before its window closed.
    Expired,
}

impl TxStatus {
    /// Returns `true` when this status is terminal, i.e. no further
    /// status transition is possible from it.
    ///
    /// `Completed` and `PartiallyRefunded` are *not* final even though
    /// the payment itself has succeeded, because a (further) refund can
    /// still follow.
    pub fn is_final(&self) -> bool {
        matches!(self, TxStatus::Failed | TxStatus::Refunded | TxStatus::Expired)
    }

    /// Returns `true` when a transaction currently in `self` may move
    /// to `next` under the transaction state machine.
    ///
    /// Allowed transitions:
    ///
    /// | From               | To                                |
    /// |--------------------|-----------------------------------|
    /// | `Pending`          | `Processing`, `Failed`, `Expired` |
    /// | `Processing`       | `Completed`, `Failed`             |
    /// | `Completed`        | `Refunded`, `PartiallyRefunded`   |
    /// | `PartiallyRefunded`| `Refunded`                        |
    /// | `Failed`           | — (final)                         |
    /// | `Refunded`         | — (final)                         |
    /// | `Expired`          | — (final)                         |
    ///
    /// Staying in the same status is never a transition and returns
    /// `false`.
    pub fn can_transition_to(&self, next: TxStatus) -> bool {
        use TxStatus::*;

        matches!(
            (self, next),
            (Pending, Processing)
                | (Pending, Failed)
                | (Pending, Expired)
                | (Processing, Completed)
                | (Processing, Failed)
                | (Completed, Refunded)
                | (Completed, PartiallyRefunded)
                | (PartiallyRefunded, Refunded)
        )
    }
}

/// A single payment transaction belonging to a merchant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique identifier of this transaction within MiraclePay.
    pub id: String,
    /// Identifier of the merchant who receives the payment.
    pub merchant_id: String,
    /// Identifier of the gateway that processes the payment
    /// (for example `"bkash"`, `"stripe"`, or `"cod"`).
    pub gateway: String,
    /// Amount of the transaction, in the transaction's currency.
    pub amount: Money,
    /// Current lifecycle status; see [`TxStatus`].
    pub status: TxStatus,
    /// Email address of the paying customer, when known.
    pub customer_email: Option<String>,
    /// Phone number of the paying customer, when known.
    pub customer_phone: Option<String>,
    /// Transaction identifier assigned by the upstream provider,
    /// once the gateway has created the payment on its side.
    pub provider_trx_id: Option<String>,
    /// Number the payment was sent from, for sender-verified
    /// gateways (for example a mobile-wallet sender number), when
    /// known.
    pub sender_number: Option<String>,
    /// Merchant- or customer-supplied reference for this payment
    /// (for example an order number), when supplied.
    pub reference: Option<String>,
    /// Free-form structured data attached to the transaction.
    pub metadata: serde_json::Value,
    /// When the transaction was created (UTC).
    pub created_at: DateTime<Utc>,
    /// When the transaction was last updated (UTC).
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::TxStatus;
    use super::TxStatus::*;

    /// Every status, for exhaustive matrix-style assertions.
    const ALL: [TxStatus; 7] = [
        Pending,
        Processing,
        Completed,
        Failed,
        Refunded,
        PartiallyRefunded,
        Expired,
    ];

    #[test]
    fn final_statuses_are_failed_refunded_and_expired() {
        assert!(Failed.is_final());
        assert!(Refunded.is_final());
        assert!(Expired.is_final());
    }

    #[test]
    fn non_final_statuses_are_pending_processing_completed_and_partially_refunded() {
        assert!(!Pending.is_final());
        assert!(!Processing.is_final());
        assert!(!Completed.is_final());
        assert!(!PartiallyRefunded.is_final());
    }

    #[test]
    fn pending_can_start_processing_fail_or_expire() {
        assert!(Pending.can_transition_to(Processing));
        assert!(Pending.can_transition_to(Failed));
        assert!(Pending.can_transition_to(Expired));
    }

    #[test]
    fn pending_cannot_skip_processing_or_refund() {
        assert!(!Pending.can_transition_to(Completed));
        assert!(!Pending.can_transition_to(Refunded));
        assert!(!Pending.can_transition_to(PartiallyRefunded));
    }

    #[test]
    fn processing_can_complete_or_fail() {
        assert!(Processing.can_transition_to(Completed));
        assert!(Processing.can_transition_to(Failed));
    }

    #[test]
    fn processing_cannot_go_backwards_expire_or_refund() {
        assert!(!Processing.can_transition_to(Pending));
        assert!(!Processing.can_transition_to(Expired));
        assert!(!Processing.can_transition_to(Refunded));
        assert!(!Processing.can_transition_to(PartiallyRefunded));
    }

    #[test]
    fn completed_can_be_fully_or_partially_refunded() {
        assert!(Completed.can_transition_to(Refunded));
        assert!(Completed.can_transition_to(PartiallyRefunded));
    }

    #[test]
    fn completed_cannot_fail_expire_or_restart() {
        assert!(!Completed.can_transition_to(Failed));
        assert!(!Completed.can_transition_to(Expired));
        assert!(!Completed.can_transition_to(Pending));
        assert!(!Completed.can_transition_to(Processing));
    }

    #[test]
    fn partially_refunded_can_only_become_fully_refunded() {
        assert!(PartiallyRefunded.can_transition_to(Refunded));
        for next in ALL {
            if next != Refunded {
                assert!(
                    !PartiallyRefunded.can_transition_to(next),
                    "PartiallyRefunded must not transition to {next:?}"
                );
            }
        }
    }

    #[test]
    fn final_statuses_have_no_outgoing_transitions() {
        for from in [Failed, Refunded, Expired] {
            assert!(from.is_final());
            for next in ALL {
                assert!(
                    !from.can_transition_to(next),
                    "{from:?} is final and must not transition to {next:?}"
                );
            }
        }
    }

    #[test]
    fn no_status_transitions_to_itself() {
        for status in ALL {
            assert!(
                !status.can_transition_to(status),
                "{status:?} -> {status:?} is not a transition"
            );
        }
    }

    #[test]
    fn full_happy_path_lifecycle_is_allowed_step_by_step() {
        // Pending -> Processing -> Completed -> PartiallyRefunded -> Refunded
        let path = [Pending, Processing, Completed, PartiallyRefunded, Refunded];
        for pair in path.windows(2) {
            assert!(
                pair[0].can_transition_to(pair[1]),
                "{:?} -> {:?} should be allowed",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn status_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&TxStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&TxStatus::PartiallyRefunded).unwrap(),
            "\"partially_refunded\""
        );
        assert_eq!(
            serde_json::from_str::<TxStatus>("\"completed\"").unwrap(),
            TxStatus::Completed
        );
    }
}
