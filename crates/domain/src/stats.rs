//! Dashboard statistics and merchant balances for MiraclePay.
//!
//! Original clean-room implementation for MiraclePay v0.0.1.
//!
//! This module is pure aggregation: it derives read-only summary
//! values ([`DashboardStats`] and [`MerchantBalance`]) from slices of
//! transactions, refunds, and settlements that the caller has already
//! loaded. It performs no I/O, mutates nothing, and never converts
//! between currencies — every sum is restricted to a single currency
//! supplied by the caller, and amounts in any other currency are
//! ignored rather than mixed in.
//!
//! All money totals are integer minor units (see [`crate::money`]),
//! and sums use saturating arithmetic so a pathological input slice
//! saturates at the `i64` bounds instead of overflowing or panicking.

use serde::{Deserialize, Serialize};

use crate::money::Currency;
use crate::refund::RefundStatus;
use crate::settlement::SettlementStatus;
use crate::transaction::TxStatus;

/// Aggregated transaction statistics for a dashboard, in one currency.
///
/// Counts are over the whole transaction slice passed to
/// [`compute_stats`]; money totals are over only the entries in
/// [`DashboardStats::currency`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardStats {
    /// Total number of transactions in the input slice, all statuses.
    pub total_transactions: u64,
    /// Number of transactions with status [`TxStatus::Completed`].
    pub completed_transactions: u64,
    /// Number of transactions with status [`TxStatus::Pending`].
    pub pending_transactions: u64,
    /// Number of transactions with status [`TxStatus::Failed`].
    pub failed_transactions: u64,
    /// Gross captured volume in minor units: the sum of `amount` over
    /// transactions with status [`TxStatus::Completed`] or
    /// [`TxStatus::PartiallyRefunded`] whose amount is in `currency`.
    /// A partially refunded payment still captured its full amount, so
    /// it counts toward gross; refunds are reported separately in
    /// `refunded_minor` and are not netted out here.
    pub gross_volume_minor: i64,
    /// Total refunded in minor units: the sum of `amount` over refunds
    /// with status [`RefundStatus::Succeeded`] whose amount is in
    /// `currency`. Pending, processing, and failed refunds have not
    /// (yet) returned money, so they do not count.
    pub refunded_minor: i64,
    /// Share of decided payments that succeeded, in basis points
    /// (10_000 bps = 100%): `completed * 10_000 / (completed + failed)`,
    /// where `completed` and `failed` are the counts above. It is `0`
    /// when there are no completed or failed transactions, so callers
    /// never divide by zero.
    pub success_rate_bps: u32,
    /// The currency every money total in this struct is denominated in.
    pub currency: crate::money::Currency,
}

/// Compute [`DashboardStats`] for `currency` from `transactions` and
/// `refunds`.
///
/// Status counts cover every transaction in the slice regardless of
/// currency; money totals include only amounts denominated in
/// `currency`. See [`DashboardStats`] for the exact definition of
/// each field.
pub fn compute_stats(
    transactions: &[crate::transaction::Transaction],
    refunds: &[crate::refund::Refund],
    currency: Currency,
) -> DashboardStats {
    let mut completed_transactions: u64 = 0;
    let mut pending_transactions: u64 = 0;
    let mut failed_transactions: u64 = 0;
    let mut gross_volume_minor: i64 = 0;

    for tx in transactions {
        match tx.status {
            TxStatus::Completed => {
                completed_transactions += 1;
                if tx.amount.currency == currency {
                    gross_volume_minor = gross_volume_minor.saturating_add(tx.amount.amount_minor);
                }
            }
            TxStatus::PartiallyRefunded => {
                if tx.amount.currency == currency {
                    gross_volume_minor = gross_volume_minor.saturating_add(tx.amount.amount_minor);
                }
            }
            TxStatus::Pending => pending_transactions += 1,
            TxStatus::Failed => failed_transactions += 1,
            // Processing, Refunded, and Expired transactions have no
            // dedicated counter and do not count toward gross volume;
            // they are still part of `total_transactions` below.
            TxStatus::Processing | TxStatus::Refunded | TxStatus::Expired => {}
        }
    }

    let mut refunded_minor: i64 = 0;
    for refund in refunds {
        if refund.status == RefundStatus::Succeeded && refund.amount.currency == currency {
            refunded_minor = refunded_minor.saturating_add(refund.amount.amount_minor);
        }
    }

    let decided = completed_transactions + failed_transactions;
    let success_rate_bps: u32 = if decided == 0 {
        0
    } else {
        // u128 keeps `completed * 10_000` exact for any u64 count; the
        // quotient is at most 10_000, so the cast to u32 cannot lose
        // information.
        ((completed_transactions as u128 * 10_000) / decided as u128) as u32
    };

    DashboardStats {
        total_transactions: transactions.len() as u64,
        completed_transactions,
        pending_transactions,
        failed_transactions,
        gross_volume_minor,
        refunded_minor,
        success_rate_bps,
        currency,
    }
}

/// A merchant's balance in one currency, derived by [`compute_balance`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerchantBalance {
    /// Identifier of the merchant this balance belongs to.
    pub merchant_id: String,
    /// Minor units available to the merchant: captured by completed
    /// transactions, minus succeeded refunds, minus the net amount of
    /// settlements already paid out. May be negative if refunds and
    /// payouts exceed the completed volume in the input slices.
    pub available_minor: i64,
    /// Minor units in transactions still [`TxStatus::Pending`] for
    /// this merchant: not yet captured, so not yet available.
    pub pending_minor: i64,
    /// The currency both totals are denominated in.
    pub currency: Currency,
}

/// Compute the [`MerchantBalance`] of `merchant_id` in `currency`.
///
/// Only entries belonging to `merchant_id` and denominated in
/// `currency` are considered:
///
/// * `available_minor` sums the amounts of the merchant's
///   [`TxStatus::Completed`] transactions, subtracts the amounts of
///   their [`RefundStatus::Succeeded`] refunds, and subtracts the net
///   (`gross - fee`, see [`Settlement::net`]) of their
///   [`SettlementStatus::Paid`] settlements. A paid settlement whose
///   net cannot be computed (for example a fee in another currency)
///   is skipped rather than guessed at.
/// * `pending_minor` sums the amounts of the merchant's
///   [`TxStatus::Pending`] transactions. Transactions in any other
///   status contribute to neither total.
pub fn compute_balance(
    merchant_id: &str,
    transactions: &[crate::transaction::Transaction],
    refunds: &[crate::refund::Refund],
    settlements: &[crate::settlement::Settlement],
    currency: Currency,
) -> MerchantBalance {
    let mut available_minor: i64 = 0;
    let mut pending_minor: i64 = 0;

    for tx in transactions {
        if tx.merchant_id != merchant_id || tx.amount.currency != currency {
            continue;
        }
        match tx.status {
            TxStatus::Completed => {
                available_minor = available_minor.saturating_add(tx.amount.amount_minor);
            }
            TxStatus::Pending => {
                pending_minor = pending_minor.saturating_add(tx.amount.amount_minor);
            }
            _ => {}
        }
    }

    for refund in refunds {
        if refund.merchant_id == merchant_id
            && refund.status == RefundStatus::Succeeded
            && refund.amount.currency == currency
        {
            available_minor = available_minor.saturating_sub(refund.amount.amount_minor);
        }
    }

    for settlement in settlements {
        if settlement.merchant_id != merchant_id
            || settlement.status != SettlementStatus::Paid
            || settlement.gross.currency != currency
        {
            continue;
        }
        if let Ok(net) = settlement.net() {
            if net.currency == currency {
                available_minor = available_minor.saturating_sub(net.amount_minor);
            }
        }
    }

    MerchantBalance {
        merchant_id: merchant_id.to_string(),
        available_minor,
        pending_minor,
        currency,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::refund::Refund;
    use crate::settlement::Settlement;
    use crate::transaction::Transaction;
    use chrono::{DateTime, Utc};

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(secs, 0).unwrap()
    }

    fn tx(
        id: &str,
        merchant_id: &str,
        amount_minor: i64,
        currency: Currency,
        status: TxStatus,
    ) -> Transaction {
        Transaction {
            id: id.to_string(),
            merchant_id: merchant_id.to_string(),
            gateway: "bkash".to_string(),
            amount: Money::new(amount_minor, currency),
            status,
            customer_email: None,
            customer_phone: None,
            provider_trx_id: None,
            sender_number: None,
            reference: None,
            metadata: serde_json::Value::Null,
            created_at: at(1_700_000_000),
            updated_at: at(1_700_000_000),
        }
    }

    fn refund(
        id: &str,
        merchant_id: &str,
        amount_minor: i64,
        currency: Currency,
        status: RefundStatus,
    ) -> Refund {
        Refund {
            id: id.to_string(),
            transaction_id: "tx_1".to_string(),
            merchant_id: merchant_id.to_string(),
            amount: Money::new(amount_minor, currency),
            reason: None,
            status,
            provider_refund_id: None,
            created_at: at(1_700_000_000),
        }
    }

    fn settlement(
        id: &str,
        merchant_id: &str,
        gross_minor: i64,
        fee_minor: i64,
        currency: Currency,
        status: SettlementStatus,
    ) -> Settlement {
        let mut s = Settlement::new(
            id,
            merchant_id,
            Money::new(gross_minor, currency),
            Money::new(fee_minor, currency),
            at(1_700_000_000),
            at(1_700_086_400),
            "bank_transfer",
        );
        s.status = status;
        s
    }

    // ---- compute_stats: counts and gross volume ---------------------

    #[test]
    fn empty_inputs_give_zero_stats() {
        let stats = compute_stats(&[], &[], Currency::BDT);
        assert_eq!(stats.total_transactions, 0);
        assert_eq!(stats.completed_transactions, 0);
        assert_eq!(stats.pending_transactions, 0);
        assert_eq!(stats.failed_transactions, 0);
        assert_eq!(stats.gross_volume_minor, 0);
        assert_eq!(stats.refunded_minor, 0);
        assert_eq!(stats.success_rate_bps, 0);
        assert_eq!(stats.currency, Currency::BDT);
    }

    #[test]
    fn counts_are_by_status_and_total_covers_every_status() {
        let txs = vec![
            tx("1", "mch_1", 100, Currency::BDT, TxStatus::Completed),
            tx("2", "mch_1", 100, Currency::BDT, TxStatus::Completed),
            tx("3", "mch_1", 100, Currency::BDT, TxStatus::Pending),
            tx("4", "mch_1", 100, Currency::BDT, TxStatus::Failed),
            tx("5", "mch_1", 100, Currency::BDT, TxStatus::Processing),
            tx("6", "mch_1", 100, Currency::BDT, TxStatus::Expired),
            tx("7", "mch_1", 100, Currency::BDT, TxStatus::Refunded),
        ];
        let stats = compute_stats(&txs, &[], Currency::BDT);
        assert_eq!(stats.total_transactions, 7);
        assert_eq!(stats.completed_transactions, 2);
        assert_eq!(stats.pending_transactions, 1);
        assert_eq!(stats.failed_transactions, 1);
    }

    #[test]
    fn gross_counts_completed_and_partially_refunded_in_currency_only() {
        let txs = vec![
            tx("1", "mch_1", 10_000, Currency::BDT, TxStatus::Completed),
            tx(
                "2",
                "mch_1",
                5_000,
                Currency::BDT,
                TxStatus::PartiallyRefunded,
            ),
            // Wrong currency: counted in totals, excluded from gross.
            tx("3", "mch_1", 7_000, Currency::USD, TxStatus::Completed),
            // Not captured statuses: excluded from gross.
            tx("4", "mch_1", 9_000, Currency::BDT, TxStatus::Pending),
            tx("5", "mch_1", 9_000, Currency::BDT, TxStatus::Failed),
            tx("6", "mch_1", 9_000, Currency::BDT, TxStatus::Refunded),
        ];
        let stats = compute_stats(&txs, &[], Currency::BDT);
        assert_eq!(stats.gross_volume_minor, 15_000);
        // The USD view sees only the USD completed transaction.
        let usd = compute_stats(&txs, &[], Currency::USD);
        assert_eq!(usd.gross_volume_minor, 7_000);
        assert_eq!(usd.total_transactions, stats.total_transactions);
    }

    #[test]
    fn refunded_counts_succeeded_refunds_in_currency_only() {
        let refunds = vec![
            refund("r1", "mch_1", 1_500, Currency::BDT, RefundStatus::Succeeded),
            refund("r2", "mch_1", 500, Currency::BDT, RefundStatus::Succeeded),
            refund("r3", "mch_1", 900, Currency::BDT, RefundStatus::Pending),
            refund("r4", "mch_1", 900, Currency::BDT, RefundStatus::Processing),
            refund("r5", "mch_1", 900, Currency::BDT, RefundStatus::Failed),
            refund("r6", "mch_1", 900, Currency::USD, RefundStatus::Succeeded),
        ];
        let stats = compute_stats(&[], &refunds, Currency::BDT);
        assert_eq!(stats.refunded_minor, 2_000);
    }

    // ---- compute_stats: success rate --------------------------------

    #[test]
    fn success_rate_is_completed_over_completed_plus_failed_in_bps() {
        let txs = vec![
            tx("1", "mch_1", 100, Currency::BDT, TxStatus::Completed),
            tx("2", "mch_1", 100, Currency::BDT, TxStatus::Completed),
            tx("3", "mch_1", 100, Currency::BDT, TxStatus::Completed),
            tx("4", "mch_1", 100, Currency::BDT, TxStatus::Failed),
            // Pending transactions are not decided and do not dilute the rate.
            tx("5", "mch_1", 100, Currency::BDT, TxStatus::Pending),
        ];
        // 3 / (3 + 1) = 75% = 7_500 bps.
        assert_eq!(
            compute_stats(&txs, &[], Currency::BDT).success_rate_bps,
            7_500
        );
    }

    #[test]
    fn success_rate_truncates_and_handles_extremes() {
        let one_of_three = vec![
            tx("1", "mch_1", 100, Currency::BDT, TxStatus::Completed),
            tx("2", "mch_1", 100, Currency::BDT, TxStatus::Failed),
            tx("3", "mch_1", 100, Currency::BDT, TxStatus::Failed),
        ];
        // 1/3 in bps truncates to 3_333, never rounds up.
        assert_eq!(
            compute_stats(&one_of_three, &[], Currency::BDT).success_rate_bps,
            3_333
        );

        let all_failed = vec![tx("1", "mch_1", 100, Currency::BDT, TxStatus::Failed)];
        assert_eq!(
            compute_stats(&all_failed, &[], Currency::BDT).success_rate_bps,
            0
        );

        let all_completed = vec![tx("1", "mch_1", 100, Currency::BDT, TxStatus::Completed)];
        assert_eq!(
            compute_stats(&all_completed, &[], Currency::BDT).success_rate_bps,
            10_000
        );

        // No decided transactions at all: 0, not a division by zero.
        let only_pending = vec![tx("1", "mch_1", 100, Currency::BDT, TxStatus::Pending)];
        assert_eq!(
            compute_stats(&only_pending, &[], Currency::BDT).success_rate_bps,
            0
        );
    }

    #[test]
    fn stats_round_trip_through_serde_json() {
        let txs = vec![tx("1", "mch_1", 250, Currency::EUR, TxStatus::Completed)];
        let stats = compute_stats(&txs, &[], Currency::EUR);
        let json = serde_json::to_string(&stats).unwrap();
        assert_eq!(
            serde_json::from_str::<DashboardStats>(&json).unwrap(),
            stats
        );
    }

    // ---- compute_balance --------------------------------------------

    #[test]
    fn balance_is_completed_minus_succeeded_refunds_minus_paid_settlements() {
        let txs = vec![
            tx("1", "mch_1", 10_000, Currency::BDT, TxStatus::Completed),
            tx("2", "mch_1", 5_000, Currency::BDT, TxStatus::Completed),
            tx("3", "mch_1", 2_000, Currency::BDT, TxStatus::Pending),
            tx("4", "mch_1", 9_000, Currency::BDT, TxStatus::Failed),
        ];
        let refunds = vec![
            refund("r1", "mch_1", 1_000, Currency::BDT, RefundStatus::Succeeded),
            // Not succeeded: must not reduce the balance.
            refund("r2", "mch_1", 700, Currency::BDT, RefundStatus::Pending),
        ];
        let settlements = vec![
            // Net 4_000 - 100 = 3_900 already paid out.
            settlement(
                "s1",
                "mch_1",
                4_000,
                100,
                Currency::BDT,
                SettlementStatus::Paid,
            ),
            // Not paid: must not reduce the balance.
            settlement(
                "s2",
                "mch_1",
                8_000,
                0,
                Currency::BDT,
                SettlementStatus::Pending,
            ),
        ];
        let balance = compute_balance("mch_1", &txs, &refunds, &settlements, Currency::BDT);
        assert_eq!(balance.merchant_id, "mch_1");
        assert_eq!(balance.currency, Currency::BDT);
        // 15_000 completed - 1_000 refunded - 3_900 paid out.
        assert_eq!(balance.available_minor, 10_100);
        assert_eq!(balance.pending_minor, 2_000);
    }

    #[test]
    fn balance_ignores_other_merchants_and_other_currencies() {
        let txs = vec![
            tx("1", "mch_1", 10_000, Currency::BDT, TxStatus::Completed),
            tx("2", "mch_2", 99_000, Currency::BDT, TxStatus::Completed),
            tx("3", "mch_1", 99_000, Currency::USD, TxStatus::Completed),
            tx("4", "mch_2", 99_000, Currency::BDT, TxStatus::Pending),
            tx("5", "mch_1", 99_000, Currency::USD, TxStatus::Pending),
        ];
        let refunds = vec![
            refund(
                "r1",
                "mch_2",
                99_000,
                Currency::BDT,
                RefundStatus::Succeeded,
            ),
            refund(
                "r2",
                "mch_1",
                99_000,
                Currency::USD,
                RefundStatus::Succeeded,
            ),
        ];
        let settlements = vec![
            settlement(
                "s1",
                "mch_2",
                99_000,
                0,
                Currency::BDT,
                SettlementStatus::Paid,
            ),
            settlement(
                "s2",
                "mch_1",
                99_000,
                0,
                Currency::USD,
                SettlementStatus::Paid,
            ),
        ];
        let balance = compute_balance("mch_1", &txs, &refunds, &settlements, Currency::BDT);
        assert_eq!(balance.available_minor, 10_000);
        assert_eq!(balance.pending_minor, 0);
    }

    #[test]
    fn balance_for_unknown_merchant_is_zero() {
        let txs = vec![tx("1", "mch_1", 10_000, Currency::BDT, TxStatus::Completed)];
        let balance = compute_balance("mch_nobody", &txs, &[], &[], Currency::BDT);
        assert_eq!(balance.merchant_id, "mch_nobody");
        assert_eq!(balance.available_minor, 0);
        assert_eq!(balance.pending_minor, 0);
    }

    #[test]
    fn balance_can_go_negative_when_refunds_exceed_completed_volume() {
        let txs = vec![tx("1", "mch_1", 1_000, Currency::BDT, TxStatus::Completed)];
        let refunds = vec![refund(
            "r1",
            "mch_1",
            1_500,
            Currency::BDT,
            RefundStatus::Succeeded,
        )];
        let balance = compute_balance("mch_1", &txs, &refunds, &[], Currency::BDT);
        assert_eq!(balance.available_minor, -500);
    }

    #[test]
    fn balance_round_trips_through_serde_json() {
        let balance = compute_balance("mch_1", &[], &[], &[], Currency::USD);
        let json = serde_json::to_string(&balance).unwrap();
        assert_eq!(
            serde_json::from_str::<MerchantBalance>(&json).unwrap(),
            balance
        );
    }
}
