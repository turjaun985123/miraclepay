//! Settlement computation job for MiraclePay v0.0.1.
//!
//! Original clean-room implementation. This module is the pure,
//! storage-free core of the periodic settlement run: given the
//! transactions that exist for a period it works out how much each
//! merchant is owed ([`collect_candidates`]), turns those totals into
//! [`Settlement`] records with the platform fee deducted
//! ([`build_settlements`]), and provides the small state helpers the
//! payout step needs ([`mark_paid`], [`total_net`]).
//!
//! Nothing here talks to a database, a clock (beyond the timestamps
//! already on the domain records), or a payout provider, so every
//! function is deterministic for a given input and can be unit-tested
//! in isolation. A caller (cron task, CLI, admin action) is expected
//! to load the transactions, persist the settlements this module
//! builds, and perform the actual payouts itself.
//!
//! All amounts are integer counts of a currency's minor unit, matching
//! the domain [`Money`](miraclepay_domain::Money) convention, so no
//! floating-point rounding ever enters settlement arithmetic.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use miraclepay_domain::errors::DomainError;
use miraclepay_domain::money::{Currency, Money};
use miraclepay_domain::settlement::{compute_fee, Settlement, SettlementStatus};
use miraclepay_domain::transaction::{Transaction, TxStatus};

/// Payout method recorded on settlements built by this job.
///
/// The job itself never moves money, so settlements are created for a
/// manual payout: an operator (or a later, provider-specific job)
/// performs the transfer and then records its reference through
/// [`mark_paid`].
pub const MANUAL_PAYOUT_METHOD: &str = "manual";

/// The gross total owed to one merchant in one currency for a period.
///
/// Produced by [`collect_candidates`] and consumed by
/// [`build_settlements`]. `gross_minor` is always strictly positive:
/// groups whose summed amount is zero or negative are dropped during
/// collection, since there is nothing to pay out for them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SettlementCandidate {
    /// Merchant the grouped transactions belong to.
    pub merchant_id: String,
    /// Sum of the grouped transaction amounts, in minor units.
    pub gross_minor: i64,
    /// Currency every grouped amount is denominated in.
    pub currency: Currency,
}

/// Collect the per-merchant, per-currency gross totals for a period.
///
/// A transaction contributes to the totals only when **all** of the
/// following hold:
///
/// - its status is [`TxStatus::Completed`] — pending, processing,
///   failed, refunded, partially refunded, and expired transactions
///   have not (fully) captured funds and never settle here;
/// - its `created_at` lies in the half-open period
///   `[period_start, period_end)` — a transaction exactly at
///   `period_start` is included, one exactly at `period_end` belongs
///   to the next period;
/// - its id is not present in `already_settled_tx_ids`, so a
///   transaction that an earlier settlement run already paid out is
///   never counted twice.
///
/// Contributing amounts are grouped by `(merchant_id, currency)` and
/// summed. Groups whose sum is not strictly positive are dropped.
/// The result is sorted by merchant id, then by currency code, so the
/// output order is deterministic for a given input. Summation uses
/// saturating arithmetic, so a pathological input cannot overflow.
pub fn collect_candidates(
    transactions: &[Transaction],
    already_settled_tx_ids: &HashSet<String>,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Vec<SettlementCandidate> {
    let mut candidates: Vec<SettlementCandidate> = Vec::new();

    for tx in transactions {
        if tx.status != TxStatus::Completed {
            continue;
        }
        if tx.created_at < period_start || tx.created_at >= period_end {
            continue;
        }
        if already_settled_tx_ids.contains(&tx.id) {
            continue;
        }

        match candidates.iter_mut().find(|candidate| {
            candidate.merchant_id == tx.merchant_id && candidate.currency == tx.amount.currency
        }) {
            Some(candidate) => {
                candidate.gross_minor =
                    candidate.gross_minor.saturating_add(tx.amount.amount_minor);
            }
            None => candidates.push(SettlementCandidate {
                merchant_id: tx.merchant_id.clone(),
                gross_minor: tx.amount.amount_minor,
                currency: tx.amount.currency,
            }),
        }
    }

    candidates.retain(|candidate| candidate.gross_minor > 0);
    candidates.sort_by(|a, b| {
        a.merchant_id
            .cmp(&b.merchant_id)
            .then_with(|| a.currency.code().cmp(b.currency.code()))
    });
    candidates
}

/// Turn settlement candidates into [`Settlement`] records.
///
/// One settlement is built per candidate, in the order the candidates
/// are given. For each candidate the platform fee is computed with
/// the domain [`compute_fee`] at `fee_bps` basis points
/// (1 bp = 0.01%, so 250 bps = 2.5%) and both the gross and the fee
/// are recorded as [`Money`] in the candidate's currency; the net
/// payout is always derived later via [`Settlement::net`].
///
/// Every settlement starts in [`SettlementStatus::Pending`] with the
/// payout method `"manual"` and no payout reference.
///
/// Settlement ids are unique within the batch: the id for the
/// candidate at position `index` is
/// `format!("{id_prefix}_{merchant_id}_{index}")`. Including the
/// merchant id keeps ids recognisable in logs, and including the
/// index keeps them unique even when one merchant settles in several
/// currencies in the same run. Callers should choose an `id_prefix`
/// that identifies the run (for example `"stl_2026_11_26"`).
pub fn build_settlements(
    candidates: &[SettlementCandidate],
    fee_bps: u32,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    id_prefix: &str,
) -> Vec<Settlement> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let fee_minor = compute_fee(candidate.gross_minor, fee_bps);
            Settlement::new(
                format!("{}_{}_{}", id_prefix, candidate.merchant_id, index),
                candidate.merchant_id.clone(),
                Money::new(candidate.gross_minor, candidate.currency),
                Money::new(fee_minor, candidate.currency),
                period_start,
                period_end,
                MANUAL_PAYOUT_METHOD,
            )
        })
        .collect()
}

/// Record that a settlement has been paid out.
///
/// Only a settlement that is [`SettlementStatus::Pending`] (computed,
/// payout not yet started) or [`SettlementStatus::Processing`]
/// (payout already handed to a provider) may be marked paid. On
/// success the settlement moves to [`SettlementStatus::Paid`] and
/// `payout_reference` is stored as its payout reference, replacing
/// any reference recorded earlier.
///
/// # Errors
/// Returns [`DomainError::InvalidStateTransition`] when the
/// settlement is in any other status (`Paid`, `Failed`, or
/// `OnHold`): paying an already-paid settlement again would pay the
/// merchant twice, and a failed or held settlement must be resolved
/// through its own path first. On error the settlement is unchanged.
pub fn mark_paid(settlement: &mut Settlement, payout_reference: &str) -> Result<(), DomainError> {
    match settlement.status {
        SettlementStatus::Pending | SettlementStatus::Processing => {
            settlement.status = SettlementStatus::Paid;
            settlement.payout_reference = Some(payout_reference.to_string());
            Ok(())
        }
        status => Err(DomainError::InvalidStateTransition(format!(
            "settlement {} cannot be marked paid from status {status:?}",
            settlement.id
        ))),
    }
}

/// Sum the net amounts (gross minus fee, in minor units) of a batch
/// of settlements.
///
/// A settlement whose net cannot be computed — for example because
/// its gross and fee are in different currencies — contributes `0`
/// rather than failing the whole total, so one corrupt record cannot
/// block a payout summary. The sum saturates at the `i64` bounds
/// instead of overflowing.
pub fn total_net(settlements: &[Settlement]) -> i64 {
    settlements.iter().fold(0i64, |total, settlement| {
        let net_minor = settlement.net().map(|net| net.amount_minor).unwrap_or(0);
        total.saturating_add(net_minor)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Period used by most tests: one day starting at a fixed instant.
    fn period_start() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn period_end() -> DateTime<Utc> {
        period_start() + chrono::Duration::days(1)
    }

    fn at(secs_from_start: i64) -> DateTime<Utc> {
        period_start() + chrono::Duration::seconds(secs_from_start)
    }

    fn tx(
        id: &str,
        merchant_id: &str,
        amount_minor: i64,
        currency: Currency,
        status: TxStatus,
        created_at: DateTime<Utc>,
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
            created_at,
            updated_at: created_at,
        }
    }

    fn completed(
        id: &str,
        merchant_id: &str,
        amount_minor: i64,
        currency: Currency,
        created_at: DateTime<Utc>,
    ) -> Transaction {
        tx(
            id,
            merchant_id,
            amount_minor,
            currency,
            TxStatus::Completed,
            created_at,
        )
    }

    fn candidate(merchant_id: &str, gross_minor: i64, currency: Currency) -> SettlementCandidate {
        SettlementCandidate {
            merchant_id: merchant_id.to_string(),
            gross_minor,
            currency,
        }
    }

    fn no_settled() -> HashSet<String> {
        HashSet::new()
    }

    // ---- collect_candidates: grouping --------------------------------

    #[test]
    fn groups_by_merchant_and_currency_and_sums_amounts() {
        let txs = vec![
            completed("tx_1", "mch_a", 10_000, Currency::BDT, at(60)),
            completed("tx_2", "mch_a", 5_000, Currency::BDT, at(120)),
            completed("tx_3", "mch_a", 7_000, Currency::USD, at(180)),
            completed("tx_4", "mch_b", 2_000, Currency::BDT, at(240)),
            completed("tx_5", "mch_b", 3_500, Currency::BDT, at(300)),
        ];

        let candidates = collect_candidates(&txs, &no_settled(), period_start(), period_end());

        // Sorted by merchant, then currency code: BDT before USD.
        assert_eq!(
            candidates,
            vec![
                candidate("mch_a", 15_000, Currency::BDT),
                candidate("mch_a", 7_000, Currency::USD),
                candidate("mch_b", 5_500, Currency::BDT),
            ]
        );
    }

    #[test]
    fn non_completed_transactions_never_contribute() {
        let txs = vec![
            tx(
                "tx_1",
                "mch_a",
                10_000,
                Currency::BDT,
                TxStatus::Pending,
                at(60),
            ),
            tx(
                "tx_2",
                "mch_a",
                10_000,
                Currency::BDT,
                TxStatus::Processing,
                at(60),
            ),
            tx(
                "tx_3",
                "mch_a",
                10_000,
                Currency::BDT,
                TxStatus::Failed,
                at(60),
            ),
            tx(
                "tx_4",
                "mch_a",
                10_000,
                Currency::BDT,
                TxStatus::Refunded,
                at(60),
            ),
            tx(
                "tx_5",
                "mch_a",
                10_000,
                Currency::BDT,
                TxStatus::PartiallyRefunded,
                at(60),
            ),
            tx(
                "tx_6",
                "mch_a",
                10_000,
                Currency::BDT,
                TxStatus::Expired,
                at(60),
            ),
        ];

        assert!(collect_candidates(&txs, &no_settled(), period_start(), period_end()).is_empty());
    }

    #[test]
    fn groups_summing_to_zero_or_negative_are_dropped() {
        let txs = vec![
            completed("tx_1", "mch_zero", 5_000, Currency::BDT, at(60)),
            completed("tx_2", "mch_zero", -5_000, Currency::BDT, at(120)),
            completed("tx_3", "mch_neg", -1_000, Currency::BDT, at(180)),
            completed("tx_4", "mch_ok", 1_000, Currency::BDT, at(240)),
        ];

        let candidates = collect_candidates(&txs, &no_settled(), period_start(), period_end());

        assert_eq!(candidates, vec![candidate("mch_ok", 1_000, Currency::BDT)]);
    }

    // ---- collect_candidates: period ----------------------------------

    #[test]
    fn period_is_start_inclusive_and_end_exclusive() {
        let txs = vec![
            // One second before the period: excluded.
            completed("tx_before", "mch_a", 1_000, Currency::BDT, at(-1)),
            // Exactly at period_start: included.
            completed("tx_at_start", "mch_a", 2_000, Currency::BDT, period_start()),
            // Last second of the period: included.
            completed(
                "tx_last_second",
                "mch_a",
                4_000,
                Currency::BDT,
                period_end() - chrono::Duration::seconds(1),
            ),
            // Exactly at period_end: belongs to the next period.
            completed("tx_at_end", "mch_a", 8_000, Currency::BDT, period_end()),
            // Well after the period: excluded.
            completed("tx_after", "mch_a", 16_000, Currency::BDT, at(90_000)),
        ];

        let candidates = collect_candidates(&txs, &no_settled(), period_start(), period_end());

        assert_eq!(
            candidates,
            vec![candidate("mch_a", 6_000, Currency::BDT)],
            "only the at-start and last-second transactions count"
        );
    }

    // ---- collect_candidates: already settled -------------------------

    #[test]
    fn already_settled_transactions_are_excluded() {
        let txs = vec![
            completed("tx_1", "mch_a", 10_000, Currency::BDT, at(60)),
            completed("tx_2", "mch_a", 5_000, Currency::BDT, at(120)),
            completed("tx_3", "mch_b", 7_000, Currency::BDT, at(180)),
        ];
        let already: HashSet<String> = ["tx_1".to_string(), "tx_3".to_string()]
            .into_iter()
            .collect();

        let candidates = collect_candidates(&txs, &already, period_start(), period_end());

        // tx_1 is skipped so mch_a keeps only tx_2; mch_b's only
        // transaction was already settled, so mch_b disappears.
        assert_eq!(candidates, vec![candidate("mch_a", 5_000, Currency::BDT)]);
    }

    #[test]
    fn empty_input_collects_nothing() {
        assert!(collect_candidates(&[], &no_settled(), period_start(), period_end()).is_empty());
    }

    // ---- build_settlements -------------------------------------------

    #[test]
    fn build_applies_fee_at_250_bps_and_starts_pending_manual() {
        let candidates = vec![candidate("mch_a", 100_000, Currency::BDT)];

        let settlements =
            build_settlements(&candidates, 250, period_start(), period_end(), "stl_run1");

        assert_eq!(settlements.len(), 1);
        let s = &settlements[0];
        // 2.5% of 100_000 minor is 2_500 minor; net is 97_500 minor.
        assert_eq!(s.gross, Money::new(100_000, Currency::BDT));
        assert_eq!(s.fee, Money::new(2_500, Currency::BDT));
        assert_eq!(s.net().unwrap(), Money::new(97_500, Currency::BDT));
        assert_eq!(s.status, SettlementStatus::Pending);
        assert_eq!(s.payout_method, "manual");
        assert_eq!(s.payout_reference, None);
        assert_eq!(s.merchant_id, "mch_a");
        assert_eq!(s.period_start, period_start());
        assert_eq!(s.period_end, period_end());
    }

    #[test]
    fn build_ids_are_unique_per_candidate_even_for_one_merchant() {
        let candidates = vec![
            candidate("mch_a", 10_000, Currency::BDT),
            candidate("mch_a", 20_000, Currency::USD),
            candidate("mch_b", 30_000, Currency::BDT),
        ];

        let settlements =
            build_settlements(&candidates, 250, period_start(), period_end(), "stl_run1");

        let ids: Vec<&str> = settlements.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["stl_run1_mch_a_0", "stl_run1_mch_a_1", "stl_run1_mch_b_2"]
        );
        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len());
        // Each settlement keeps its own candidate's currency and fee.
        assert_eq!(settlements[1].gross, Money::new(20_000, Currency::USD));
        assert_eq!(settlements[1].fee, Money::new(500, Currency::USD));
        assert_eq!(settlements[2].fee, Money::new(750, Currency::BDT));
    }

    #[test]
    fn build_with_no_candidates_builds_nothing() {
        assert!(build_settlements(&[], 250, period_start(), period_end(), "stl_x").is_empty());
    }

    // ---- mark_paid -----------------------------------------------------

    fn pending_settlement() -> Settlement {
        build_settlements(
            &[candidate("mch_a", 10_000, Currency::BDT)],
            250,
            period_start(),
            period_end(),
            "stl_run1",
        )
        .into_iter()
        .next()
        .unwrap()
    }

    #[test]
    fn mark_paid_from_pending_sets_paid_and_reference() {
        let mut s = pending_settlement();

        mark_paid(&mut s, "bank_ref_123").unwrap();

        assert_eq!(s.status, SettlementStatus::Paid);
        assert_eq!(s.payout_reference.as_deref(), Some("bank_ref_123"));
    }

    #[test]
    fn mark_paid_from_processing_sets_paid_and_reference() {
        let mut s = pending_settlement();
        s.status = SettlementStatus::Processing;

        mark_paid(&mut s, "bkash_trx_9").unwrap();

        assert_eq!(s.status, SettlementStatus::Paid);
        assert_eq!(s.payout_reference.as_deref(), Some("bkash_trx_9"));
    }

    #[test]
    fn mark_paid_from_paid_is_an_invalid_transition() {
        let mut s = pending_settlement();
        mark_paid(&mut s, "bank_ref_123").unwrap();

        let err = mark_paid(&mut s, "bank_ref_456").unwrap_err();

        assert!(
            matches!(err, DomainError::InvalidStateTransition(_)),
            "expected InvalidStateTransition, got {err:?}"
        );
        // The failed call changes nothing: still paid, original ref.
        assert_eq!(s.status, SettlementStatus::Paid);
        assert_eq!(s.payout_reference.as_deref(), Some("bank_ref_123"));
    }

    #[test]
    fn mark_paid_from_failed_and_on_hold_is_invalid() {
        for status in [SettlementStatus::Failed, SettlementStatus::OnHold] {
            let mut s = pending_settlement();
            s.status = status;

            let err = mark_paid(&mut s, "bank_ref_123").unwrap_err();

            assert!(
                matches!(err, DomainError::InvalidStateTransition(_)),
                "expected InvalidStateTransition from {status:?}, got {err:?}"
            );
            assert_eq!(s.status, status);
            assert_eq!(s.payout_reference, None);
        }
    }

    // ---- total_net -----------------------------------------------------

    #[test]
    fn total_net_sums_net_amounts() {
        let settlements = build_settlements(
            &[
                candidate("mch_a", 100_000, Currency::BDT),
                candidate("mch_b", 10_000, Currency::BDT),
            ],
            250,
            period_start(),
            period_end(),
            "stl_run1",
        );

        // (100_000 - 2_500) + (10_000 - 250) = 97_500 + 9_750.
        assert_eq!(total_net(&settlements), 107_250);
    }

    #[test]
    fn total_net_ignores_settlements_whose_net_errors() {
        let mut settlements = build_settlements(
            &[candidate("mch_a", 100_000, Currency::BDT)],
            250,
            period_start(),
            period_end(),
            "stl_run1",
        );
        // A settlement whose fee is in another currency has no
        // computable net; it must contribute 0, not fail the total.
        let broken = Settlement::new(
            "stl_broken",
            "mch_b",
            Money::new(50_000, Currency::BDT),
            Money::new(1_000, Currency::USD),
            period_start(),
            period_end(),
            "manual",
        );
        settlements.push(broken);

        assert_eq!(total_net(&settlements), 97_500);
    }

    #[test]
    fn total_net_of_nothing_is_zero() {
        assert_eq!(total_net(&[]), 0);
    }

    // ---- end to end ----------------------------------------------------

    #[test]
    fn collect_build_and_pay_end_to_end() {
        let txs = vec![
            completed("tx_1", "mch_a", 40_000, Currency::BDT, at(60)),
            completed("tx_2", "mch_a", 60_000, Currency::BDT, at(120)),
            completed("tx_3", "mch_b", 10_000, Currency::USD, at(180)),
        ];

        let candidates = collect_candidates(&txs, &no_settled(), period_start(), period_end());
        let mut settlements =
            build_settlements(&candidates, 250, period_start(), period_end(), "stl_e2e");

        assert_eq!(settlements.len(), 2);
        assert_eq!(total_net(&settlements), 97_500 + 9_750);

        for s in &mut settlements {
            mark_paid(s, "payout_ref").unwrap();
            assert_eq!(s.status, SettlementStatus::Paid);
        }
    }
}
