//! Bangladesh gateway SMS detection for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! This module answers two questions about an incoming SMS:
//!
//! 1. Which Bangladesh mobile-money gateway did it come from?
//!    ([`detect_bd_gateway`]) — bKash, Nagad, Rocket, Upay, or
//!    SureCash.
//! 2. Does it look like a payment notification rather than some
//!    other message from the same sender? ([`is_bd_payment_sms`])
//!
//! Detection is deliberately simple and dependency-free: gateway
//! sender IDs are short alphanumeric labels (for example `"bKash"`),
//! so a case-insensitive substring match against a small keyword
//! table is both sufficient and easy to audit. The sender ID is
//! authoritative; the message body is consulted only as a fallback
//! when the sender is not recognised, because a body can mention a
//! gateway it did not come from (a merchant's own confirmation text,
//! a forwarded message, and so on).

use miraclepay_gateways::types::GatewayId;

/// Sender-ID / body keywords for each Bangladesh gateway, paired
/// with the gateway they identify.
///
/// Keywords are matched case-insensitively as substrings, so they
/// are stored in lowercase. Rocket is Dutch-Bangla Bank's mobile
/// banking service, hence the `"dbbl"` alias; SureCash messages
/// have also been seen under the `"mycash"` brand, hence its alias.
const BD_GATEWAY_KEYWORDS: &[(&str, GatewayId)] = &[
    ("bkash", GatewayId::Bkash),
    ("nagad", GatewayId::Nagad),
    ("rocket", GatewayId::Rocket),
    ("dbbl", GatewayId::Rocket),
    ("upay", GatewayId::Upay),
    ("surecash", GatewayId::Surecash),
    ("mycash", GatewayId::Surecash),
];

/// Body keywords that mark an SMS as a payment notification: money
/// arriving in the account, as opposed to an OTP, a balance enquiry,
/// a cash-out, or a promotional message.
///
/// Stored in lowercase; matching is case-insensitive substring
/// matching, so `"cash in"` also covers spellings such as
/// `"Cash In"` and `"CASH IN"`.
const BD_PAYMENT_KEYWORDS: &[&str] = &["received", "payment", "cash in", "credited"];

/// Known sender IDs used by Bangladesh gateways, in their canonical
/// brand spelling.
///
/// This is the list a handset-side forwarder or an admin screen can
/// show as "senders we recognise". Detection itself uses the
/// keyword table, so decorated variants of these IDs (for example
/// `"16247"` short-code companions or `"BKASH-ALERT"`) are still
/// recognised by [`detect_bd_gateway`] even though they are not
/// listed here verbatim.
const BD_SENDER_IDS: &[&str] = &[
    "bKash",
    "Nagad",
    "Rocket",
    "DBBL",
    "Upay",
    "SureCash",
    "MyCash",
];

/// The known sender IDs of Bangladesh gateways.
///
/// Returns the canonical brand spellings; see [`BD_SENDER_IDS`].
pub fn bd_sender_ids() -> &'static [&'static str] {
    BD_SENDER_IDS
}

/// Find the first gateway whose keyword appears in `haystack`.
///
/// `haystack` must already be lowercased; every keyword in
/// [`BD_GATEWAY_KEYWORDS`] is lowercase.
fn gateway_for_keyword(haystack: &str) -> Option<GatewayId> {
    BD_GATEWAY_KEYWORDS
        .iter()
        .find(|(keyword, _)| haystack.contains(keyword))
        .map(|(_, gateway)| *gateway)
}

/// Detect which Bangladesh gateway an SMS came from.
///
/// The sender ID is checked first, case-insensitively:
///
/// - `"bkash"` → [`GatewayId::Bkash`]
/// - `"nagad"` → [`GatewayId::Nagad`]
/// - `"rocket"` or `"dbbl"` → [`GatewayId::Rocket`]
/// - `"upay"` → [`GatewayId::Upay`]
/// - `"surecash"` or `"mycash"` → [`GatewayId::Surecash`]
///
/// Only if the sender ID matches no gateway is the body consulted,
/// with the same keywords (so a body mentioning "bKash", "Nagad",
/// and so on identifies the gateway when the sender is an unknown
/// number or short code). A sender that does match always wins over
/// whatever the body mentions.
///
/// Returns `None` when neither the sender nor the body identifies a
/// Bangladesh gateway.
pub fn detect_bd_gateway(sender_id: &str, body: &str) -> Option<GatewayId> {
    let sender_lower = sender_id.to_ascii_lowercase();
    if let Some(gateway) = gateway_for_keyword(&sender_lower) {
        return Some(gateway);
    }

    let body_lower = body.to_ascii_lowercase();
    gateway_for_keyword(&body_lower)
}

/// Whether an SMS looks like a Bangladesh gateway payment
/// notification.
///
/// True only when both conditions hold:
///
/// 1. [`detect_bd_gateway`] identifies a Bangladesh gateway from
///    the sender ID (or, failing that, the body), and
/// 2. the body contains a payment keyword — `"received"`,
///    `"payment"`, `"cash in"`, or `"credited"` —
///    case-insensitively.
///
/// This intentionally rejects other messages from a gateway sender,
/// such as OTP codes ("Your OTP is ..."), balance notifications,
/// and promotions, which carry no payment keyword.
pub fn is_bd_payment_sms(sender_id: &str, body: &str) -> bool {
    if detect_bd_gateway(sender_id, body).is_none() {
        return false;
    }

    let body_lower = body.to_ascii_lowercase();
    BD_PAYMENT_KEYWORDS
        .iter()
        .any(|keyword| body_lower.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- detect_bd_gateway: by sender ID ---------------------------

    #[test]
    fn detects_bkash_by_sender_id() {
        assert_eq!(
            detect_bd_gateway("bKash", "You have received Tk 500.00"),
            Some(GatewayId::Bkash)
        );
    }

    #[test]
    fn detects_nagad_by_sender_id() {
        assert_eq!(
            detect_bd_gateway("Nagad", "You have received Tk 500.00"),
            Some(GatewayId::Nagad)
        );
    }

    #[test]
    fn detects_rocket_by_sender_id() {
        assert_eq!(
            detect_bd_gateway("Rocket", "You have received Tk 500.00"),
            Some(GatewayId::Rocket)
        );
    }

    #[test]
    fn detects_rocket_by_dbbl_sender_id() {
        assert_eq!(
            detect_bd_gateway("DBBL", "You have received Tk 500.00"),
            Some(GatewayId::Rocket)
        );
    }

    #[test]
    fn detects_upay_by_sender_id() {
        assert_eq!(
            detect_bd_gateway("Upay", "You have received Tk 500.00"),
            Some(GatewayId::Upay)
        );
    }

    #[test]
    fn detects_surecash_by_sender_id() {
        assert_eq!(
            detect_bd_gateway("SureCash", "You have received Tk 500.00"),
            Some(GatewayId::Surecash)
        );
    }

    #[test]
    fn detects_surecash_by_mycash_sender_id() {
        assert_eq!(
            detect_bd_gateway("MyCash", "You have received Tk 500.00"),
            Some(GatewayId::Surecash)
        );
    }

    #[test]
    fn sender_id_matching_is_case_insensitive() {
        assert_eq!(detect_bd_gateway("BKASH", ""), Some(GatewayId::Bkash));
        assert_eq!(detect_bd_gateway("nagad", ""), Some(GatewayId::Nagad));
        assert_eq!(detect_bd_gateway("ROCKET", ""), Some(GatewayId::Rocket));
        assert_eq!(detect_bd_gateway("uPaY", ""), Some(GatewayId::Upay));
        assert_eq!(
            detect_bd_gateway("SURECASH", ""),
            Some(GatewayId::Surecash)
        );
    }

    // --- detect_bd_gateway: body fallback --------------------------

    #[test]
    fn falls_back_to_body_when_sender_is_unknown() {
        assert_eq!(
            detect_bd_gateway("16247", "bKash: You have received Tk 500.00"),
            Some(GatewayId::Bkash)
        );
        assert_eq!(
            detect_bd_gateway("Unknown", "Nagad payment received: Tk 250.00"),
            Some(GatewayId::Nagad)
        );
        assert_eq!(
            detect_bd_gateway("8801000000000", "Your Rocket account was credited"),
            Some(GatewayId::Rocket)
        );
        assert_eq!(
            detect_bd_gateway("Unknown", "Upay Cash In successful"),
            Some(GatewayId::Upay)
        );
        assert_eq!(
            detect_bd_gateway("Unknown", "SureCash payment received"),
            Some(GatewayId::Surecash)
        );
    }

    #[test]
    fn sender_id_wins_over_a_different_gateway_named_in_the_body() {
        // The body mentions Nagad, but the sender is bKash: the
        // sender is authoritative and the body is not consulted.
        assert_eq!(
            detect_bd_gateway("bKash", "Your Nagad reference number is 123"),
            Some(GatewayId::Bkash)
        );
    }

    // --- detect_bd_gateway: no match -------------------------------

    #[test]
    fn unrelated_sms_detects_no_gateway() {
        assert_eq!(
            detect_bd_gateway("Grameenphone", "Your internet pack has been activated"),
            None
        );
        assert_eq!(detect_bd_gateway("MOM", "Dinner is at 8 tonight"), None);
        assert_eq!(detect_bd_gateway("", ""), None);
    }

    // --- bd_sender_ids ---------------------------------------------

    #[test]
    fn bd_sender_ids_lists_every_gateway_brand() {
        let ids = bd_sender_ids();
        for expected in ["bKash", "Nagad", "Rocket", "DBBL", "Upay", "SureCash", "MyCash"] {
            assert!(ids.contains(&expected), "missing sender id {expected}");
        }
    }

    #[test]
    fn every_listed_sender_id_detects_a_gateway() {
        for sender_id in bd_sender_ids() {
            assert!(
                detect_bd_gateway(sender_id, "").is_some(),
                "listed sender id {sender_id} detects no gateway"
            );
        }
    }

    // --- is_bd_payment_sms -----------------------------------------

    #[test]
    fn payment_sms_with_received_keyword_is_a_payment() {
        assert!(is_bd_payment_sms(
            "bKash",
            "You have received Tk 500.00 from 01700000000. TrxID 9HX12345"
        ));
    }

    #[test]
    fn payment_sms_with_payment_keyword_is_a_payment() {
        assert!(is_bd_payment_sms(
            "Nagad",
            "Payment of Tk 250.00 successful. Txn ID: NAG123"
        ));
    }

    #[test]
    fn payment_sms_with_cash_in_keyword_is_a_payment() {
        assert!(is_bd_payment_sms("Upay", "Cash In Tk 1,000.00 successful"));
    }

    #[test]
    fn payment_sms_with_credited_keyword_is_a_payment() {
        assert!(is_bd_payment_sms(
            "DBBL",
            "Your account has been credited with Tk 750.00"
        ));
    }

    #[test]
    fn payment_keywords_are_case_insensitive() {
        assert!(is_bd_payment_sms("bKash", "TK 500 RECEIVED"));
        assert!(is_bd_payment_sms("Nagad", "PAYMENT successful"));
    }

    #[test]
    fn otp_sms_is_not_a_payment() {
        assert!(!is_bd_payment_sms(
            "bKash",
            "Your OTP is 483920. Do not share it with anyone."
        ));
        assert!(!is_bd_payment_sms(
            "Nagad",
            "Your OTP is 112233 for verification."
        ));
    }

    #[test]
    fn gateway_sms_without_a_payment_keyword_is_not_a_payment() {
        assert!(!is_bd_payment_sms(
            "bKash",
            "Your current balance is Tk 1,240.50"
        ));
    }

    #[test]
    fn payment_keyword_without_a_gateway_is_not_a_payment() {
        assert!(!is_bd_payment_sms(
            "MOM",
            "I received your payment, thanks!"
        ));
    }

    #[test]
    fn unrelated_sms_is_not_a_payment() {
        assert!(!is_bd_payment_sms(
            "Grameenphone",
            "Your internet pack has been activated"
        ));
    }
}
