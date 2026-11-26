//! Matching a parsed SMS to a pending transaction, for MiraclePay.
//!
//! Original clean-room implementation for MiraclePay v0.0.1.
//!
//! When a payment-confirmation SMS arrives (forwarded by the Android
//! app or received from a gateway short code), the verification
//! engine has already turned its text into a [`ParsedSms`]. This
//! module decides which — if any — still-open [`Transaction`] that
//! SMS is evidence for, so the caller can settle exactly one
//! transaction and never settle the same SMS twice.
//!
//! The decision is deliberately conservative and explainable:
//!
//! 1. A transaction id that has already been consumed by an earlier
//!    SMS is a [`MatchOutcome::Duplicate`], checked before anything
//!    else so a replayed SMS can never match a second transaction.
//! 2. Only transactions still awaiting payment —
//!    [`TxStatus::Pending`] or [`TxStatus::Processing`] — are
//!    candidates. Completed, failed, refunded, and expired
//!    transactions are invisible to the matcher.
//! 3. Each candidate is scored by [`score_candidate`]. A score of at
//!    least 80 is an [`MatchOutcome::Exact`] match; at least 50 with
//!    a confirmed amount and currency is a
//!    [`MatchOutcome::Probable`] match; anything else is
//!    [`MatchOutcome::NoMatch`].
//!
//! Amounts are compared in minor units only, and only when the SMS
//! actually carried an amount: a missing amount never matches.

use std::collections::HashSet;

use miraclepay_domain::transaction::{Transaction, TxStatus};

use crate::types::ParsedSms;

/// Score at or above which a candidate is an exact match.
const EXACT_THRESHOLD: u32 = 80;
/// Score at or above which a candidate (with amount and currency
/// confirmed) is a probable match.
const PROBABLE_THRESHOLD: u32 = 50;

/// The outcome of trying to match one parsed SMS against the set of
/// pending transactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchOutcome {
    /// The SMS certainly belongs to the transaction with this id:
    /// either the provider transaction id matches, or the amount and
    /// the sender number both match.
    Exact(String),
    /// The SMS probably belongs to the transaction with this id:
    /// the amount and currency match, but the sender differs or was
    /// not present in the SMS, so a human (or a further check)
    /// should confirm before settling.
    Probable(String),
    /// No pending transaction is a plausible owner of this SMS.
    NoMatch,
    /// This SMS's transaction id has already been used to verify an
    /// earlier payment; the SMS must not be applied again.
    Duplicate,
}

/// Whether `tx` is still open enough for an SMS to settle it.
fn is_candidate(tx: &Transaction) -> bool {
    matches!(tx.status, TxStatus::Pending | TxStatus::Processing)
}

/// Whether two optional strings are both present, non-blank, and
/// equal after trimming surrounding whitespace.
fn both_present_and_equal(a: &Option<String>, b: &Option<String>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            let (a, b) = (a.trim(), b.trim());
            !a.is_empty() && a == b
        }
        _ => false,
    }
}

/// Whether the SMS amount is present and equals the transaction
/// amount, in minor units.
fn amount_matches(parsed: &ParsedSms, tx: &Transaction) -> bool {
    match parsed.amount_minor {
        Some(amount_minor) => amount_minor == tx.amount.amount_minor,
        None => false,
    }
}

/// Whether the SMS currency is present and equals the transaction
/// currency.
fn currency_matches(parsed: &ParsedSms, tx: &Transaction) -> bool {
    parsed.currency == Some(tx.amount.currency)
}

/// Score how strongly `parsed` points at `tx`.
///
/// Points are additive, one award per independent signal:
///
/// | Signal                                            | Points |
/// |---------------------------------------------------|--------|
/// | Provider transaction id matches                   |    100 |
/// | Amount matches (minor units)                      |     50 |
/// | Sender number matches                             |     30 |
/// | Receiver number or gateway matches                |     20 |
///
/// The receiver/gateway award is given when the SMS gateway (when
/// known) names the transaction's gateway, compared
/// case-insensitively, or when the SMS receiver number (when known)
/// equals the transaction's customer phone number. The awards are
/// chosen so the thresholds in [`match_transaction`] line up with
/// the match rules: a transaction-id match alone (100), or amount
/// plus sender (50 + 30 = 80), reach the exact threshold, while an
/// amount match on its own (50) reaches only the probable
/// threshold.
pub fn score_candidate(parsed: &ParsedSms, tx: &Transaction) -> u32 {
    let mut score: u32 = 0;

    // Provider transaction id: the strongest possible signal.
    if both_present_and_equal(&parsed.trx_id, &tx.provider_trx_id) {
        score += 100;
    }

    if amount_matches(parsed, tx) {
        score += 50;
    }

    if both_present_and_equal(&parsed.sender_number, &tx.sender_number) {
        score += 30;
    }

    // Receiver / gateway: either signal earns the same award, once.
    let gateway_matches = parsed
        .gateway
        .is_some_and(|gateway| gateway.as_str().eq_ignore_ascii_case(tx.gateway.trim()));
    let receiver_matches =
        both_present_and_equal(&parsed.receiver_number, &tx.customer_phone);
    if gateway_matches || receiver_matches {
        score += 20;
    }

    score
}

/// Match `parsed` against `pending`, honouring `already_used_trx`
/// (the provider transaction ids already consumed by earlier SMS
/// verifications).
///
/// Rules, in order:
///
/// 1. If the SMS carries a transaction id that is in
///    `already_used_trx`, the result is
///    [`MatchOutcome::Duplicate`], regardless of the candidates.
/// 2. Candidates are the transactions in `pending` whose status is
///    [`TxStatus::Pending`] or [`TxStatus::Processing`].
/// 3. A candidate is *exact* when its score (see
///    [`score_candidate`]) is at least 80 — in practice a matching
///    provider transaction id, or a matching amount together with a
///    matching sender number. If several candidates are exact, the
///    oldest one by `created_at` wins, so a repeated payment settles
///    the longest-waiting transaction first.
/// 4. Otherwise a candidate is *probable* when its score is at
///    least 50 and both its amount and its currency match the SMS
///    (the sender differing or being absent is exactly what keeps
///    it probable rather than exact). The highest-scoring probable
///    candidate wins; ties break towards the oldest `created_at`.
/// 5. Otherwise the result is [`MatchOutcome::NoMatch`].
pub fn match_transaction(
    parsed: &ParsedSms,
    pending: &[Transaction],
    already_used_trx: &HashSet<String>,
) -> MatchOutcome {
    // 1. Replay guard: a transaction id may verify one payment only.
    if let Some(trx_id) = parsed.trx_id.as_deref().map(str::trim) {
        if !trx_id.is_empty() && already_used_trx.contains(trx_id) {
            return MatchOutcome::Duplicate;
        }
    }

    let candidates: Vec<&Transaction> = pending.iter().filter(|tx| is_candidate(tx)).collect();

    // 3. Exact matches: highest confidence, oldest first.
    let mut exact: Vec<&Transaction> = candidates
        .iter()
        .copied()
        .filter(|tx| score_candidate(parsed, tx) >= EXACT_THRESHOLD)
        .collect();
    exact.sort_by_key(|tx| tx.created_at);
    if let Some(tx) = exact.first() {
        return MatchOutcome::Exact(tx.id.clone());
    }

    // 4. Probable matches: amount and currency must both be
    // confirmed; the sender is what is missing or different.
    let mut probable: Vec<(u32, &Transaction)> = candidates
        .iter()
        .copied()
        .filter(|tx| amount_matches(parsed, tx) && currency_matches(parsed, tx))
        .map(|tx| (score_candidate(parsed, tx), tx))
        .filter(|(score, _)| *score >= PROBABLE_THRESHOLD)
        .collect();
    // Highest score first, oldest first within the same score.
    probable.sort_by(|(score_a, a), (score_b, b)| {
        score_b.cmp(score_a).then_with(|| a.created_at.cmp(&b.created_at))
    });
    if let Some((_, tx)) = probable.first() {
        return MatchOutcome::Probable(tx.id.clone());
    }

    MatchOutcome::NoMatch
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use miraclepay_domain::money::{Currency, Money};
    use miraclepay_gateways::types::GatewayId;

    const SENDER: &str = "01711000000";
    const OTHER_SENDER: &str = "01822000000";
    const RECEIVER: &str = "01933000000";

    fn at(hour: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 26, hour, 0, 0).unwrap()
    }

    fn tx(id: &str, amount_minor: i64, created_hour: u32) -> Transaction {
        Transaction {
            id: id.to_string(),
            merchant_id: "merchant-1".to_string(),
            gateway: "bkash".to_string(),
            amount: Money::new(amount_minor, Currency::BDT),
            status: TxStatus::Pending,
            customer_email: None,
            customer_phone: Some(RECEIVER.to_string()),
            provider_trx_id: None,
            sender_number: Some(SENDER.to_string()),
            reference: None,
            metadata: serde_json::Value::Null,
            created_at: at(created_hour),
            updated_at: at(created_hour),
        }
    }

    fn sms() -> ParsedSms {
        ParsedSms {
            gateway: Some(GatewayId::Bkash),
            sender_number: Some(SENDER.to_string()),
            receiver_number: Some(RECEIVER.to_string()),
            amount_minor: Some(150_000),
            currency: Some(Currency::BDT),
            trx_id: Some("TRX-ABC123".to_string()),
            reference: None,
            balance_minor: None,
            raw_sender: "bKash".to_string(),
            raw_body: "You have received Tk 1,500.00 from 01711000000. TrxID TRX-ABC123"
                .to_string(),
            received_at: at(10),
        }
    }

    fn no_used() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn exact_by_trx_id_even_when_amount_and_sender_differ() {
        let mut candidate = tx("tx-1", 999_999, 9);
        candidate.provider_trx_id = Some("TRX-ABC123".to_string());
        candidate.sender_number = Some(OTHER_SENDER.to_string());
        let mut parsed = sms();
        parsed.amount_minor = Some(1);
        parsed.receiver_number = None;
        parsed.gateway = None;

        assert_eq!(score_candidate(&parsed, &candidate), 100);
        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::Exact("tx-1".to_string())
        );
    }

    #[test]
    fn exact_by_amount_and_sender_without_trx_id() {
        let candidate = tx("tx-2", 150_000, 9);
        let mut parsed = sms();
        parsed.trx_id = None;
        parsed.receiver_number = None;
        parsed.gateway = None;

        // 50 (amount) + 30 (sender) = 80, exactly the threshold.
        assert_eq!(score_candidate(&parsed, &candidate), 80);
        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::Exact("tx-2".to_string())
        );
    }

    #[test]
    fn probable_by_amount_and_currency_only() {
        let candidate = tx("tx-3", 150_000, 9);
        let mut parsed = sms();
        parsed.trx_id = None;
        parsed.sender_number = Some(OTHER_SENDER.to_string());
        parsed.receiver_number = None;
        parsed.gateway = None;

        // Amount matches, currency matches, sender differs.
        assert_eq!(score_candidate(&parsed, &candidate), 50);
        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::Probable("tx-3".to_string())
        );
    }

    #[test]
    fn probable_when_sender_is_absent() {
        let candidate = tx("tx-4", 150_000, 9);
        let mut parsed = sms();
        parsed.trx_id = None;
        parsed.sender_number = None;
        parsed.receiver_number = None;
        parsed.gateway = None;

        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::Probable("tx-4".to_string())
        );
    }

    #[test]
    fn duplicate_when_trx_id_already_used() {
        let mut candidate = tx("tx-5", 150_000, 9);
        candidate.provider_trx_id = Some("TRX-ABC123".to_string());
        let parsed = sms();
        let mut used = HashSet::new();
        used.insert("TRX-ABC123".to_string());

        assert_eq!(
            match_transaction(&parsed, &[candidate], &used),
            MatchOutcome::Duplicate
        );
    }

    #[test]
    fn duplicate_is_checked_even_with_no_candidates() {
        let parsed = sms();
        let mut used = HashSet::new();
        used.insert("TRX-ABC123".to_string());

        assert_eq!(match_transaction(&parsed, &[], &used), MatchOutcome::Duplicate);
    }

    #[test]
    fn no_match_when_nothing_is_pending() {
        let parsed = sms();
        assert_eq!(match_transaction(&parsed, &[], &no_used()), MatchOutcome::NoMatch);
    }

    #[test]
    fn no_match_for_wrong_amount() {
        let candidate = tx("tx-6", 140_000, 9);
        let mut parsed = sms();
        parsed.trx_id = None;

        // Sender matches (30) and gateway matches (20), but without
        // the amount neither threshold that matters is reachable.
        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::NoMatch
        );
    }

    #[test]
    fn no_match_when_amount_is_missing_from_sms() {
        let candidate = tx("tx-7", 150_000, 9);
        let mut parsed = sms();
        parsed.trx_id = None;
        parsed.amount_minor = None;

        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::NoMatch
        );
    }

    #[test]
    fn amount_match_in_wrong_currency_is_not_probable() {
        let candidate = tx("tx-8", 150_000, 9);
        let mut parsed = sms();
        parsed.trx_id = None;
        parsed.sender_number = Some(OTHER_SENDER.to_string());
        parsed.currency = Some(Currency::INR);
        parsed.receiver_number = None;
        parsed.gateway = None;

        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::NoMatch
        );
    }

    #[test]
    fn completed_transactions_are_not_candidates() {
        let mut candidate = tx("tx-9", 150_000, 9);
        candidate.status = TxStatus::Completed;
        candidate.provider_trx_id = Some("TRX-ABC123".to_string());
        let parsed = sms();

        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::NoMatch
        );
    }

    #[test]
    fn processing_transactions_are_candidates() {
        let mut candidate = tx("tx-10", 150_000, 9);
        candidate.status = TxStatus::Processing;
        let mut parsed = sms();
        parsed.trx_id = None;
        parsed.receiver_number = None;
        parsed.gateway = None;

        assert_eq!(
            match_transaction(&parsed, &[candidate], &no_used()),
            MatchOutcome::Exact("tx-10".to_string())
        );
    }

    #[test]
    fn oldest_exact_candidate_wins() {
        let newer = tx("tx-new", 150_000, 12);
        let older = tx("tx-old", 150_000, 8);
        let mut parsed = sms();
        parsed.trx_id = None;
        parsed.receiver_number = None;
        parsed.gateway = None;

        assert_eq!(
            match_transaction(&parsed, &[newer, older], &no_used()),
            MatchOutcome::Exact("tx-old".to_string())
        );
    }

    #[test]
    fn score_candidate_awards_each_signal_once() {
        let mut candidate = tx("tx-11", 150_000, 9);
        candidate.provider_trx_id = Some("TRX-ABC123".to_string());
        let parsed = sms();

        // 100 (trx) + 50 (amount) + 30 (sender) + 20 (gateway/receiver).
        assert_eq!(score_candidate(&parsed, &candidate), 200);

        let unrelated = tx("tx-12", 1, 9);
        let mut parsed = sms();
        parsed.trx_id = Some("TRX-OTHER".to_string());
        parsed.sender_number = Some(OTHER_SENDER.to_string());
        parsed.receiver_number = None;
        parsed.gateway = Some(GatewayId::Nagad);
        assert_eq!(score_candidate(&parsed, &unrelated), 0);
    }
}
