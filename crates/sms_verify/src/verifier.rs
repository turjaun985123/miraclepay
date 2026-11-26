//! Top-level SMS verification engine for MiraclePay v0.0.1.
//!
//! Original clean-room implementation. This module is the entry point
//! of the `miraclepay-sms-verify` crate: it combines the sibling
//! single-responsibility modules into the two operations the rest of
//! the platform actually calls.
//!
//! * [`parse_incoming`] turns a raw forwarded SMS ([`IncomingSms`])
//!   into a structured [`ParsedSms`]: it detects which gateway sent
//!   the message (Bangladeshi gateways first, then Indian / Pakistani
//!   ones), and extracts the amount, the provider transaction id, the
//!   sender's phone number and the reported balance using the generic
//!   field extractors in [`crate::parser`].
//! * [`verify_sms`] runs the full pipeline — parse, then match the
//!   parsed payment against the merchant's pending transactions with
//!   [`crate::matcher::match_transaction`] — and reduces the result
//!   to a single [`VerificationOutcome`].
//!
//! Neither function performs I/O or mutates its inputs: the caller
//! owns persisting the outcome (marking the transaction completed,
//! recording the used transaction id, and so on).

use std::collections::HashSet;

use miraclepay_domain::money::Currency;
use miraclepay_domain::transaction::Transaction;
use miraclepay_gateways::types::GatewayId;

use crate::matcher::{match_transaction, MatchOutcome};
use crate::parser::{
    extract_amount_minor, extract_balance_minor, extract_phone, extract_trx_id,
};
use crate::patterns_bd::detect_bd_gateway;
use crate::patterns_in_pk::detect_in_pk_gateway;
use crate::types::{IncomingSms, ParsedSms, SmsParseError};

/// Parse a raw incoming SMS into a structured payment record.
///
/// # Steps
///
/// 1. A blank (empty or whitespace-only) body is rejected with
///    [`SmsParseError::EmptyBody`] before any other work happens.
/// 2. The sending gateway is detected from the body, trying the
///    Bangladeshi patterns ([`detect_bd_gateway`]) first and the
///    India / Pakistan patterns ([`detect_in_pk_gateway`]) second.
///    Detection may legitimately fail (`None`): a payment SMS from an
///    unrecognised sender can still carry a usable amount and
///    transaction id.
/// 3. Amount, transaction id, sender phone and balance are extracted
///    with the generic extractors in [`crate::parser`]. The sender
///    phone is the first phone number found in the body; when the
///    body carries none, the SMS envelope sender is used instead if
///    it itself looks like a phone number (some forwarders deliver
///    the wallet's shortcode / brand name as the sender, which is
///    not a number and is never reported as one).
/// 4. The currency is derived from the detected gateway's country
///    ([`GatewayId::country`]): `BD -> BDT`, `IN -> INR`,
///    `PK -> PKR`. Without a detected gateway (or for a global
///    gateway) the currency stays `None` rather than being guessed.
///
/// # Errors
///
/// * [`SmsParseError::EmptyBody`] — the body is blank.
/// * [`SmsParseError::NotAPaymentSms`] — neither an amount nor a
///   transaction id could be found, so the message does not look
///   like a payment notification at all. Either one alone is enough
///   to accept the message: some gateways omit the amount in some
///   templates, and some omit the transaction id.
pub fn parse_incoming(sms: &IncomingSms) -> Result<ParsedSms, SmsParseError> {
    if sms.body.trim().is_empty() {
        return Err(SmsParseError::EmptyBody);
    }

    let gateway = detect_bd_gateway(&sms.sender, &sms.body)
        .or_else(|| detect_in_pk_gateway(&sms.sender, &sms.body));

    let amount_minor = extract_amount_minor(&sms.body);
    let trx_id = extract_trx_id(&sms.body);

    if amount_minor.is_none() && trx_id.is_none() {
        return Err(SmsParseError::NotAPaymentSms);
    }

    let sender_number = extract_phone(&sms.body).or_else(|| {
        if looks_like_phone_number(&sms.sender) {
            Some(sms.sender.trim().to_string())
        } else {
            None
        }
    });

    let balance_minor = extract_balance_minor(&sms.body);
    let currency = gateway.and_then(currency_for_gateway);

    Ok(ParsedSms {
        gateway,
        amount_minor,
        currency,
        sender_number,
        receiver_number: None,
        trx_id,
        reference: None,
        balance_minor,
        raw_sender: sms.sender.clone(),
        raw_body: sms.body.clone(),
        received_at: sms.received_at,
    })
}

/// The final verdict of verifying one incoming SMS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationOutcome {
    /// The SMS exactly matches a pending transaction; carries that
    /// transaction's id. The payment can be settled automatically.
    Verified(String),
    /// The SMS probably matches a pending transaction (for example
    /// amount and sender agree but the transaction id is missing or
    /// partial); carries that transaction's id. A human or a later
    /// confirmation step should settle it, not the automatic path.
    ProbableMatch(String),
    /// The SMS looks like a payment but matches no pending
    /// transaction.
    Unmatched,
    /// The SMS's transaction id has already been used to verify an
    /// earlier payment, so this copy must not settle anything again.
    Duplicate,
    /// The SMS is not a payment notification at all.
    NotPayment,
}

/// Verify one incoming SMS against a merchant's pending transactions.
///
/// The SMS is first parsed with [`parse_incoming`]; any parse failure
/// means the message is not a payment SMS and yields
/// [`VerificationOutcome::NotPayment`]. A successfully parsed payment
/// is matched with [`match_transaction`] against `pending`, skipping
/// transaction ids already present in `already_used_trx`, and the
/// [`MatchOutcome`] is mapped one-to-one:
///
/// | `MatchOutcome` | `VerificationOutcome`      |
/// |----------------|----------------------------|
/// | `Exact(id)`    | `Verified(id)`             |
/// | `Probable(id)` | `ProbableMatch(id)`        |
/// | `Duplicate`    | `Duplicate`                |
/// | `NoMatch`      | `Unmatched`                |
pub fn verify_sms(
    sms: &IncomingSms,
    pending: &[Transaction],
    already_used_trx: &HashSet<String>,
) -> VerificationOutcome {
    let parsed = match parse_incoming(sms) {
        Ok(parsed) => parsed,
        Err(_) => return VerificationOutcome::NotPayment,
    };

    match match_transaction(&parsed, pending, already_used_trx) {
        MatchOutcome::Exact(tx_id) => VerificationOutcome::Verified(tx_id),
        MatchOutcome::Probable(tx_id) => VerificationOutcome::ProbableMatch(tx_id),
        MatchOutcome::Duplicate => VerificationOutcome::Duplicate,
        MatchOutcome::NoMatch => VerificationOutcome::Unmatched,
    }
}

/// The currency payments through `gateway` are denominated in,
/// derived from the gateway's primary country
/// ([`GatewayId::country`]).
///
/// Returns `None` for global gateways, whose payments are not tied
/// to one of the launch markets' currencies.
fn currency_for_gateway(gateway: GatewayId) -> Option<Currency> {
    match gateway.country() {
        "BD" => Some(Currency::BDT),
        "IN" => Some(Currency::INR),
        "PK" => Some(Currency::PKR),
        _ => None,
    }
}

/// Whether `value` looks like a phone number rather than a brand
/// name or shortcode label: an optional leading `+` followed by
/// 6–15 digits, ignoring spaces and dashes used as separators.
///
/// Used only for the SMS envelope sender fallback in
/// [`parse_incoming`]; brand senders such as `"bKash"` return
/// `false`.
fn looks_like_phone_number(value: &str) -> bool {
    let trimmed = value.trim();
    let digits: String = trimmed
        .strip_prefix('+')
        .unwrap_or(trimmed)
        .chars()
        .filter(|c| *c != ' ' && *c != '-')
        .collect();
    (6..=15).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use miraclepay_domain::money::Money;
    use miraclepay_domain::transaction::TxStatus;

    /// An invented bKash-style "cash in / received" notification.
    /// Every value in it is fictional and made up for this test.
    const BKASH_BODY: &str = "You have received Tk 1,250.00 from 01712345678. \
        TrxID: 9HX7K2LMNP. Balance: Tk 5,430.50. Thank you for using bKash.";

    const TRX_ID: &str = "9HX7K2LMNP";
    /// 1,250.00 BDT in minor units (poisha).
    const AMOUNT_MINOR: i64 = 125_000;
    const SENDER_PHONE: &str = "01712345678";

    fn sms(sender: &str, body: &str) -> IncomingSms {
        IncomingSms {
            device_id: "test-device".to_string(),
            sender: sender.to_string(),
            body: body.to_string(),
            received_at: chrono::Utc::now(),
            sim_slot: None,
        }
    }

    fn bkash_sms() -> IncomingSms {
        sms("bKash", BKASH_BODY)
    }

    /// A pending transaction expecting exactly the payment the
    /// invented bKash SMS reports.
    fn pending_bkash_tx() -> Transaction {
        let now = chrono::Utc::now();
        Transaction {
            id: "tx_0001".to_string(),
            merchant_id: "merchant_0001".to_string(),
            gateway: "bkash".to_string(),
            amount: Money::new(AMOUNT_MINOR, Currency::BDT),
            status: TxStatus::Pending,
            customer_email: None,
            customer_phone: Some(SENDER_PHONE.to_string()),
            provider_trx_id: Some(TRX_ID.to_string()),
            sender_number: Some(SENDER_PHONE.to_string()),
            reference: Some("order_0001".to_string()),
            metadata: serde_json::Value::Null,
            created_at: now,
            updated_at: now,
        }
    }

    fn no_used_trx() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn parse_incoming_extracts_bkash_payment_fields() {
        let parsed = parse_incoming(&bkash_sms()).expect("bKash SMS should parse");

        assert_eq!(parsed.gateway, Some(GatewayId::Bkash));
        assert_eq!(parsed.amount_minor, Some(AMOUNT_MINOR));
        assert_eq!(parsed.trx_id, Some(TRX_ID.to_string()));
        assert_eq!(parsed.sender_number, Some(SENDER_PHONE.to_string()));
        // 5,430.50 BDT in minor units.
        assert_eq!(parsed.balance_minor, Some(543_050));
        assert_eq!(parsed.currency, Some(Currency::BDT));
    }

    #[test]
    fn parse_incoming_rejects_blank_body() {
        assert_eq!(
            parse_incoming(&sms("bKash", "   ")),
            Err(SmsParseError::EmptyBody)
        );
        assert_eq!(
            parse_incoming(&sms("bKash", "")),
            Err(SmsParseError::EmptyBody)
        );
    }

    #[test]
    fn parse_incoming_rejects_non_payment_sms() {
        let parsed = parse_incoming(&sms("Friend", "Meeting moved to 3pm tomorrow, see you there."));
        assert_eq!(parsed, Err(SmsParseError::NotAPaymentSms));
    }

    #[test]
    fn parse_incoming_falls_back_to_numeric_envelope_sender() {
        // The body carries an amount and a transaction id but no
        // phone number; the envelope sender is the number.
        let incoming = sms(
            SENDER_PHONE,
            "bKash: Cash In Tk 1,250.00 successful. TrxID: 9HX7K2LMNP.",
        );
        let parsed = parse_incoming(&incoming).expect("payment SMS should parse");
        assert_eq!(parsed.sender_number, Some(SENDER_PHONE.to_string()));
    }

    #[test]
    fn parse_incoming_does_not_report_brand_sender_as_phone() {
        // Same body, but the envelope sender is a brand name: it
        // must not be reported as the sender's phone number.
        let incoming = sms(
            "bKash",
            "bKash: Cash In Tk 1,250.00 successful. TrxID: 9HX7K2LMNP.",
        );
        let parsed = parse_incoming(&incoming).expect("payment SMS should parse");
        assert_eq!(parsed.sender_number, None);
    }

    #[test]
    fn verify_sms_exact_match_is_verified() {
        let pending = vec![pending_bkash_tx()];
        let outcome = verify_sms(&bkash_sms(), &pending, &no_used_trx());
        assert_eq!(
            outcome,
            VerificationOutcome::Verified("tx_0001".to_string())
        );
    }

    #[test]
    fn verify_sms_already_used_trx_is_duplicate() {
        let pending = vec![pending_bkash_tx()];
        let mut used = HashSet::new();
        used.insert(TRX_ID.to_string());
        let outcome = verify_sms(&bkash_sms(), &pending, &used);
        assert_eq!(outcome, VerificationOutcome::Duplicate);
    }

    #[test]
    fn verify_sms_without_pending_match_is_unmatched() {
        let outcome = verify_sms(&bkash_sms(), &[], &no_used_trx());
        assert_eq!(outcome, VerificationOutcome::Unmatched);
    }

    #[test]
    fn verify_sms_non_payment_is_not_payment() {
        let incoming = sms("Friend", "Meeting moved to 3pm tomorrow, see you there.");
        let pending = vec![pending_bkash_tx()];
        let outcome = verify_sms(&incoming, &pending, &no_used_trx());
        assert_eq!(outcome, VerificationOutcome::NotPayment);
    }

    #[test]
    fn verify_sms_blank_body_is_not_payment() {
        let pending = vec![pending_bkash_tx()];
        let outcome = verify_sms(&sms("bKash", "  "), &pending, &no_used_trx());
        assert_eq!(outcome, VerificationOutcome::NotPayment);
    }

    #[test]
    fn currency_follows_gateway_country() {
        assert_eq!(currency_for_gateway(GatewayId::Bkash), Some(Currency::BDT));
        assert_eq!(currency_for_gateway(GatewayId::Nagad), Some(Currency::BDT));
        assert_eq!(currency_for_gateway(GatewayId::Upi), Some(Currency::INR));
        assert_eq!(currency_for_gateway(GatewayId::Paytm), Some(Currency::INR));
        assert_eq!(
            currency_for_gateway(GatewayId::Easypaisa),
            Some(Currency::PKR)
        );
        assert_eq!(
            currency_for_gateway(GatewayId::Jazzcash),
            Some(Currency::PKR)
        );
        assert_eq!(currency_for_gateway(GatewayId::Stripe), None);
        assert_eq!(currency_for_gateway(GatewayId::Manual), None);
    }

    #[test]
    fn looks_like_phone_number_accepts_numbers_and_rejects_brands() {
        assert!(looks_like_phone_number("01712345678"));
        assert!(looks_like_phone_number("+880 1712-345678"));
        assert!(!looks_like_phone_number("bKash"));
        assert!(!looks_like_phone_number("16247"));
        assert!(!looks_like_phone_number(""));
    }
}
