//! Core SMS types for MiraclePay SMS verification.
//!
//! Original clean-room implementation for MiraclePay v0.0.1.
//!
//! Two representations of a text message live here, one for each side
//! of the verification pipeline:
//!
//! - [`IncomingSms`] is the wire format: exactly what the Android
//!   forwarder app POSTs to the server when a message arrives on a
//!   merchant's phone. It is deliberately raw — no parsing or
//!   interpretation has happened yet.
//! - [`ParsedSms`] is the result of running a parser over an
//!   [`IncomingSms`]: the fields a gateway's confirmation message is
//!   expected to carry, each optional because not every gateway (and
//!   not every message type) supplies all of them. The unparsed
//!   sender and body are kept alongside the extracted fields so a
//!   parse can always be audited or re-run with a better parser.
//!
//! Amounts are integer counts of the currency's minor unit (see
//! [`miraclepay_domain::money::Money`]), never floating point.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use miraclepay_domain::money::Currency;
use miraclepay_gateways::types::GatewayId;

/// An SMS as forwarded by the Android app: the inbound wire format.
///
/// The app is a dumb pipe — it reports what the phone received and
/// adds only the identity of the reporting device and, on dual-SIM
/// phones, which SIM slot the message arrived on. All interpretation
/// (gateway detection, amount extraction, matching against expected
/// payments) happens server-side from these fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncomingSms {
    /// Identifier of the Android device that forwarded this message.
    /// Lets the server attribute messages to a merchant's registered
    /// phone and reject forwards from unknown devices.
    pub device_id: String,
    /// The message's sender as reported by the phone: a phone number
    /// or an alphanumeric sender ID (for example a gateway's short
    /// name), exactly as received, before any normalisation.
    pub sender: String,
    /// The full message body, exactly as received.
    pub body: String,
    /// When the message was received on the device.
    pub received_at: DateTime<Utc>,
    /// Zero-based SIM slot the message arrived on, when the device
    /// reports it. `None` on single-SIM devices or when the slot is
    /// unknown.
    pub sim_slot: Option<u8>,
}

/// The fields extracted from an SMS by a gateway parser.
///
/// Every extracted field is optional: a parser fills in what the
/// message actually contains and leaves the rest `None` rather than
/// guessing. In particular, a message that is not a payment
/// confirmation at all parses to a `ParsedSms` with `amount_minor`
/// and `trx_id` both `None` — see [`ParsedSms::is_payment`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedSms {
    /// The gateway this message appears to come from, when the
    /// sender or body identifies one.
    pub gateway: Option<GatewayId>,
    /// The paid amount, in the minor unit of [`ParsedSms::currency`],
    /// when the message states one.
    pub amount_minor: Option<i64>,
    /// The currency of [`ParsedSms::amount_minor`] (and of
    /// [`ParsedSms::balance_minor`], when present), when known.
    pub currency: Option<Currency>,
    /// The payer's account / phone number, when the message names
    /// who sent the money.
    pub sender_number: Option<String>,
    /// The receiving account / phone number (typically the
    /// merchant's own number), when the message names it.
    pub receiver_number: Option<String>,
    /// The gateway's transaction identifier for this payment, when
    /// the message carries one. Together with the amount, this is
    /// what a payment is matched and deduplicated on.
    pub trx_id: Option<String>,
    /// A caller / merchant reference quoted in the message, when
    /// present, for reconciling against an order or invoice.
    pub reference: Option<String>,
    /// The account balance after this transaction, in minor units,
    /// when the message reports one. Useful as a sanity check; never
    /// used to decide the paid amount.
    pub balance_minor: Option<i64>,
    /// The sender of the original message, exactly as received.
    pub raw_sender: String,
    /// The body of the original message, exactly as received.
    pub raw_body: String,
    /// When the original message was received on the device.
    pub received_at: DateTime<Utc>,
}

impl ParsedSms {
    /// Whether this message looks like a payment confirmation.
    ///
    /// Both signals are required: an amount alone could be a balance
    /// enquiry, an advertisement, or a failed-transaction notice, and
    /// a transaction id alone carries nothing to reconcile. A message
    /// with both an amount and a transaction id is the minimum a
    /// verifier can match against an expected payment.
    pub fn is_payment(&self) -> bool {
        self.amount_minor.is_some() && self.trx_id.is_some()
    }
}

/// Why parsing an SMS failed outright.
///
/// Note that "this message is not a payment" is an error only for
/// callers that require a payment; parsers that scan a mixed inbox
/// normally represent a non-payment message as a [`ParsedSms`] with
/// no amount and no transaction id instead (see
/// [`ParsedSms::is_payment`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SmsParseError {
    /// The message body was empty or only whitespace, so there was
    /// nothing to parse.
    #[error("SMS body is empty")]
    EmptyBody,

    /// The message was understood but is not a payment SMS (for
    /// example an OTP, a promotion, or a balance notification with
    /// no transfer).
    #[error("SMS is not a payment SMS")]
    NotAPaymentSms,

    /// The message did not match any known gateway format, so no
    /// fields could be extracted with confidence.
    #[error("unrecognized SMS format")]
    UnrecognizedFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `ParsedSms` with nothing extracted, which individual tests
    /// then fill in field by field.
    fn blank_parsed() -> ParsedSms {
        ParsedSms {
            gateway: None,
            amount_minor: None,
            currency: None,
            sender_number: None,
            receiver_number: None,
            trx_id: None,
            reference: None,
            balance_minor: None,
            raw_sender: "bKash".to_string(),
            raw_body: String::new(),
            received_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        }
    }

    #[test]
    fn is_payment_requires_both_amount_and_trx_id() {
        let mut parsed = blank_parsed();
        parsed.amount_minor = Some(150_000);
        parsed.trx_id = Some("9HX7K2LM1P".to_string());
        assert!(parsed.is_payment());
    }

    #[test]
    fn is_payment_is_false_without_amount() {
        let mut parsed = blank_parsed();
        parsed.trx_id = Some("9HX7K2LM1P".to_string());
        assert!(!parsed.is_payment());
    }

    #[test]
    fn is_payment_is_false_without_trx_id() {
        let mut parsed = blank_parsed();
        parsed.amount_minor = Some(150_000);
        assert!(!parsed.is_payment());
    }

    #[test]
    fn is_payment_is_false_with_neither_amount_nor_trx_id() {
        assert!(!blank_parsed().is_payment());
    }

    #[test]
    fn is_payment_ignores_other_fields() {
        // Gateway, currency, and balance being known does not turn a
        // message with no amount / transaction id into a payment.
        let mut parsed = blank_parsed();
        parsed.gateway = Some(GatewayId::Bkash);
        parsed.currency = Some(Currency::BDT);
        parsed.balance_minor = Some(2_500_00);
        assert!(!parsed.is_payment());

        // And a zero amount still counts as an amount: presence, not
        // positivity, is the signal.
        parsed.amount_minor = Some(0);
        parsed.trx_id = Some("ZERO1".to_string());
        assert!(parsed.is_payment());
    }

    #[test]
    fn incoming_sms_serde_roundtrip_with_sim_slot() {
        let sms = IncomingSms {
            device_id: "device-42".to_string(),
            sender: "bKash".to_string(),
            body: "You have received Tk 1,500.00. TrxID 9HX7K2LM1P".to_string(),
            received_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            sim_slot: Some(1),
        };
        let json = serde_json::to_string(&sms).unwrap();
        let decoded: IncomingSms = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, sms);
    }

    #[test]
    fn incoming_sms_serde_roundtrip_without_sim_slot() {
        let sms = IncomingSms {
            device_id: "device-7".to_string(),
            sender: "+8801700000000".to_string(),
            body: "Payment received".to_string(),
            received_at: chrono::DateTime::from_timestamp(1_700_000_123, 456_000_000).unwrap(),
            sim_slot: None,
        };
        let json = serde_json::to_string(&sms).unwrap();
        // An absent SIM slot is an explicit null, not a missing key,
        // so other implementations see the same shape every time.
        assert!(json.contains("\"sim_slot\":null"), "{json}");
        let decoded: IncomingSms = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, sms);
    }

    #[test]
    fn parsed_sms_serde_roundtrip() {
        let mut parsed = blank_parsed();
        parsed.gateway = Some(GatewayId::Bkash);
        parsed.amount_minor = Some(150_000);
        parsed.currency = Some(Currency::BDT);
        parsed.trx_id = Some("9HX7K2LM1P".to_string());
        parsed.raw_body = "You have received Tk 1,500.00".to_string();
        let json = serde_json::to_string(&parsed).unwrap();
        let decoded: ParsedSms = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, parsed);
        assert!(decoded.is_payment());
    }

    #[test]
    fn sms_parse_error_display_messages() {
        assert_eq!(SmsParseError::EmptyBody.to_string(), "SMS body is empty");
        assert_eq!(
            SmsParseError::NotAPaymentSms.to_string(),
            "SMS is not a payment SMS"
        );
        assert_eq!(
            SmsParseError::UnrecognizedFormat.to_string(),
            "unrecognized SMS format"
        );
    }
}
