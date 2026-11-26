//! India + Pakistan gateway SMS detection patterns for MiraclePay.
//!
//! Original clean-room implementation for MiraclePay v0.0.1.
//!
//! This module answers one narrow question: given the sender id and
//! the body of an incoming SMS, does it look like a payment
//! notification from one of the India (IN) or Pakistan (PK) gateways
//! MiraclePay supports, and if so, which one?
//!
//! It is the IN/PK counterpart of the Bangladesh patterns module and
//! has the same shape: a single detection function, a region lookup,
//! and the list of sender ids the region is known to use. It uses
//! only `miraclepay-gateways` types and the standard library — no
//! regular expressions and no parsing of amounts, transaction ids,
//! or balances. Amount / reference extraction belongs to the caller;
//! this module only classifies.
//!
//! Detection is deliberately conservative:
//!
//! * Brand keywords (`paytm`, `phonepe`, `razorpay`, `easypaisa`,
//!   `jazzcash`, …) are matched case-insensitively against both the
//!   sender id and the body.
//! * The generic [`GatewayId::Upi`] case is stricter: a bare mention
//!   of UPI is not enough. The sender must be a known Indian bank
//!   sender and the body must mention UPI / a VPA *and* an actual
//!   payment, so bank alerts, UPI marketing, and PIN / OTP prompts
//!   are not misclassified as payments.
//! * OTP / verification-code messages are never classified, even
//!   when they arrive from a branded sender: an OTP proves a login
//!   attempt, not a payment.

use miraclepay_gateways::types::GatewayId;

/// Known sender ids for the India + Pakistan gateways and for the
/// Indian banks whose alerts can carry a UPI payment.
///
/// Sender ids are the alphanumeric headers mobile operators deliver
/// (for example `"HDFCBK"`), plus the plain brand names used when a
/// gateway sends under its own name. Matching elsewhere in this
/// module is case-insensitive, so the casing here is documentary:
/// it is the casing the operators commonly present.
const IN_PK_SENDER_IDS: &[&str] = &[
    // India — gateway brands.
    "Paytm",
    "PhonePe",
    "Razorpay",
    // India — banks whose SMS alerts report UPI payments.
    "SBIINB",
    "SBIBNK",
    "HDFCBK",
    "ICICIB",
    "AXISBK",
    "KOTAKB",
    "PNBSMS",
    "BARODA",
    "CANBNK",
    "IDBIBK",
    "YESBNK",
    "FEDBNK",
    "IOBCHN",
    "CENTBK",
    "UBOIBK",
    "BOBINB",
    "MAHABK",
    "PSBIBK",
    // Pakistan — gateway brands.
    "Easypaisa",
    "JazzCash",
];

/// Substrings (already lowercase) that identify an Indian bank
/// sender, matched against the lowercased sender id.
///
/// This is intentionally broader than [`IN_PK_SENDER_IDS`]: bank
/// sender headers vary by circle and product (`"VM-HDFCBK"`,
/// `"AD-ICICIB-S"`, …), so a bank is recognised by a distinctive
/// fragment of its header or by its name appearing in the header.
const INDIAN_BANK_SENDER_MARKERS: &[&str] = &[
    "sbi",     // State Bank of India (SBIINB, SBIBNK, …)
    "hdfc",    // HDFC Bank
    "icici",   // ICICI Bank
    "axis",    // Axis Bank
    "kotak",   // Kotak Mahindra Bank
    "pnb",     // Punjab National Bank
    "baroda",  // Bank of Baroda
    "canara",  // Canara Bank
    "canbnk",  // Canara Bank header fragment
    "idbi",    // IDBI Bank
    "yesbnk",  // Yes Bank
    "yes bank",
    "federal", // Federal Bank
    "fedbnk",  // Federal Bank header fragment
    "iob",     // Indian Overseas Bank
    "central bank",
    "centbk",  // Central Bank of India header fragment
    "union bank",
    "uboibk",  // Union Bank of India header fragment
    "bobinb",  // Bank of Baroda header fragment
    "mahabk",  // Bank of Maharashtra header fragment
    "psbibk",  // Punjab & Sind Bank header fragment
    "indian bank",
    "idfc",    // IDFC First Bank
    "rblbnk",  // RBL Bank
    "bandhan", // Bandhan Bank
    "aubank",  // AU Small Finance Bank
    "paytmbank", // Paytm Payments Bank — a bank sender, not the wallet brand alone
];

/// Words (already lowercase) that say a payment actually happened,
/// as opposed to an offer, a reminder, or a credential prompt. Used
/// only for the generic UPI case and the OTP guard.
///
/// Deliberately past-tense / amount based: the bare nouns
/// "payment(s)" and "transaction" are *not* markers, because bank
/// marketing ("Now make UPI payments with your account") uses them
/// without any payment taking place.
const PAYMENT_MARKERS: &[&str] = &[
    "paid",
    "received",
    "credited",
    "debited",
    "transferred",
    "sent",
    "rs.",
    "rs ",
    "inr",
    "₹",
];

/// Phrases (already lowercase) that mark a message as a one-time
/// password / verification-code message rather than a payment.
const OTP_MARKERS: &[&str] = &[
    "otp",
    "one-time password",
    "one time password",
    "verification code",
    "verify code",
    "do not share",
    "don't share",
];

/// Returns the known sender ids for the India + Pakistan region.
///
/// The list is a matching aid and documentation for operators, not
/// an allow-list: [`detect_in_pk_gateway`] also recognises brand
/// keywords in the message body and bank-header variants that are
/// not spelled out here.
pub fn in_pk_sender_ids() -> &'static [&'static str] {
    IN_PK_SENDER_IDS
}

/// The region a gateway belongs to, as an ISO 3166-1 alpha-2 code.
///
/// Returns `Some("IN")` for the Indian gateways
/// ([`GatewayId::Upi`], [`GatewayId::Paytm`], [`GatewayId::Phonepe`],
/// [`GatewayId::Razorpay`]), `Some("PK")` for the Pakistani gateways
/// ([`GatewayId::Easypaisa`], [`GatewayId::Jazzcash`]), and `None`
/// for every other gateway — including the Bangladesh gateways,
/// which are this module's neighbours but not its region.
pub fn region_of(gateway: GatewayId) -> Option<&'static str> {
    match gateway {
        GatewayId::Upi | GatewayId::Paytm | GatewayId::Phonepe | GatewayId::Razorpay => {
            Some("IN")
        }
        GatewayId::Easypaisa | GatewayId::Jazzcash => Some("PK"),
        _ => None,
    }
}

/// Detect an India or Pakistan gateway from an SMS sender and body.
///
/// Matching is case-insensitive over both `sender_id` and `body`:
///
/// | Keyword in sender or body          | Gateway              |
/// | ---------------------------------- | -------------------- |
/// | `paytm`                            | [`GatewayId::Paytm`] |
/// | `phonepe` or `phone pe`            | [`GatewayId::Phonepe`] |
/// | `razorpay`                         | [`GatewayId::Razorpay`] |
/// | `easypaisa` or `easy paisa`        | [`GatewayId::Easypaisa`] |
/// | `jazzcash` or `jazz cash`          | [`GatewayId::Jazzcash`] |
/// | `upi` or `vpa`, Indian bank sender, and a payment mentioned in the body | [`GatewayId::Upi`] |
///
/// Branded gateways are checked before the generic UPI case, so a
/// PhonePe or Paytm UPI notification is attributed to that brand
/// rather than to plain UPI.
///
/// Returns `None` when nothing matches — in particular for OTP /
/// verification-code messages, for Bangladesh gateways such as
/// bKash, and for bank messages that do not report a UPI payment.
pub fn detect_in_pk_gateway(sender_id: &str, body: &str) -> Option<GatewayId> {
    let sender = sender_id.to_ascii_lowercase();
    let text = body.to_ascii_lowercase();

    // An OTP is not a payment, even from a branded sender.
    if is_otp_message(&text) && !mentions_payment(&text) {
        return None;
    }

    // Branded gateways: keyword in the sender id or in the body.
    if contains(&sender, "paytm") || contains(&text, "paytm") {
        return Some(GatewayId::Paytm);
    }
    if contains(&sender, "phonepe")
        || contains(&sender, "phone pe")
        || contains(&text, "phonepe")
        || contains(&text, "phone pe")
    {
        return Some(GatewayId::Phonepe);
    }
    if contains(&sender, "razorpay") || contains(&text, "razorpay") {
        return Some(GatewayId::Razorpay);
    }
    if contains(&sender, "easypaisa")
        || contains(&sender, "easy paisa")
        || contains(&text, "easypaisa")
        || contains(&text, "easy paisa")
    {
        return Some(GatewayId::Easypaisa);
    }
    if contains(&sender, "jazzcash")
        || contains(&sender, "jazz cash")
        || contains(&text, "jazzcash")
        || contains(&text, "jazz cash")
    {
        return Some(GatewayId::Jazzcash);
    }

    // Generic UPI: bank sender + UPI/VPA mention + a real payment.
    if is_indian_bank_sender(&sender)
        && (contains(&text, "upi") || contains(&text, "vpa"))
        && mentions_payment(&text)
    {
        return Some(GatewayId::Upi);
    }

    None
}

/// Whether `haystack` contains `needle` (both already lowercase).
fn contains(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

/// Whether the (lowercased) body looks like an OTP / code message.
fn is_otp_message(lower_body: &str) -> bool {
    OTP_MARKERS.iter().any(|marker| lower_body.contains(marker))
}

/// Whether the (lowercased) body mentions an actual payment.
fn mentions_payment(lower_body: &str) -> bool {
    PAYMENT_MARKERS
        .iter()
        .any(|marker| lower_body.contains(marker))
}

/// Whether the (lowercased) sender id belongs to an Indian bank.
fn is_indian_bank_sender(lower_sender: &str) -> bool {
    INDIAN_BANK_SENDER_MARKERS
        .iter()
        .any(|marker| lower_sender.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Paytm ---------------------------------------------------

    #[test]
    fn detects_paytm_from_sender() {
        assert_eq!(
            detect_in_pk_gateway("Paytm", "Rs.500 paid to a merchant. Txn ID 123456."),
            Some(GatewayId::Paytm)
        );
    }

    #[test]
    fn detects_paytm_from_body_case_insensitively() {
        assert_eq!(
            detect_in_pk_gateway(
                "VM-WALLET",
                "PAYTM: payment of Rs.250 received in your wallet."
            ),
            Some(GatewayId::Paytm)
        );
    }

    // ---- PhonePe -------------------------------------------------

    #[test]
    fn detects_phonepe_from_sender() {
        assert_eq!(
            detect_in_pk_gateway("PhonePe", "You paid Rs.199 via UPI. UTR 123456789012."),
            Some(GatewayId::Phonepe)
        );
    }

    #[test]
    fn detects_phone_pe_with_a_space_in_body() {
        assert_eq!(
            detect_in_pk_gateway(
                "AD-ALERTS",
                "Phone Pe payment successful: Rs.350 debited from your account."
            ),
            Some(GatewayId::Phonepe)
        );
    }

    // ---- Razorpay ------------------------------------------------

    #[test]
    fn detects_razorpay_from_sender() {
        assert_eq!(
            detect_in_pk_gateway(
                "Razorpay",
                "Payment received: INR 1,499.00. Payment ID pay_123456."
            ),
            Some(GatewayId::Razorpay)
        );
    }

    #[test]
    fn detects_razorpay_from_body() {
        assert_eq!(
            detect_in_pk_gateway(
                "VM-PAYALRT",
                "Your razorpay payment of Rs.799 was successful."
            ),
            Some(GatewayId::Razorpay)
        );
    }

    // ---- UPI -----------------------------------------------------

    #[test]
    fn detects_upi_from_indian_bank_sender_and_payment_body() {
        assert_eq!(
            detect_in_pk_gateway(
                "HDFCBK",
                "Rs.1,000.00 credited to your account via UPI. UPI Ref 123456789012."
            ),
            Some(GatewayId::Upi)
        );
    }

    #[test]
    fn detects_upi_via_vpa_mention_from_bank_sender() {
        assert_eq!(
            detect_in_pk_gateway(
                "VM-ICICIB",
                "Payment of INR 450 debited. VPA merchant@okicici. Txn 987654."
            ),
            Some(GatewayId::Upi)
        );
    }

    #[test]
    fn upi_requires_an_indian_bank_sender() {
        // Same payment body, unknown sender: not enough for UPI.
        assert_eq!(
            detect_in_pk_gateway(
                "UNKNOWN",
                "Rs.1,000 credited via UPI. UPI Ref 123456789012."
            ),
            None
        );
    }

    #[test]
    fn upi_requires_a_payment_in_the_body() {
        // Bank sender + UPI mention, but no payment took place.
        assert_eq!(
            detect_in_pk_gateway(
                "SBIINB",
                "Now make UPI payments with your SBI account. Register today!"
            ),
            None
        );
    }

    #[test]
    fn branded_upi_notification_wins_over_generic_upi() {
        assert_eq!(
            detect_in_pk_gateway(
                "HDFCBK",
                "PhonePe UPI payment of Rs.120 debited from your account."
            ),
            Some(GatewayId::Phonepe)
        );
    }

    // ---- Easypaisa -----------------------------------------------

    #[test]
    fn detects_easypaisa_from_sender() {
        assert_eq!(
            detect_in_pk_gateway(
                "Easypaisa",
                "You have received PKR 5,000 in your Easypaisa account. Trx ID 123456."
            ),
            Some(GatewayId::Easypaisa)
        );
    }

    #[test]
    fn detects_easypaisa_from_body_case_insensitively() {
        assert_eq!(
            detect_in_pk_gateway(
                "3737",
                "EASYPAISA: payment of PKR 2,500 sent successfully."
            ),
            Some(GatewayId::Easypaisa)
        );
    }

    // ---- JazzCash ------------------------------------------------

    #[test]
    fn detects_jazzcash_from_sender() {
        assert_eq!(
            detect_in_pk_gateway(
                "JazzCash",
                "You have received PKR 3,000 in your JazzCash account. TID 654321."
            ),
            Some(GatewayId::Jazzcash)
        );
    }

    #[test]
    fn detects_jazzcash_from_body() {
        assert_eq!(
            detect_in_pk_gateway(
                "8558",
                "Jazz Cash transaction successful. PKR 1,200 paid to merchant."
            ),
            Some(GatewayId::Jazzcash)
        );
    }

    // ---- Negatives -----------------------------------------------

    #[test]
    fn bangladesh_sender_is_not_detected_here() {
        assert_eq!(
            detect_in_pk_gateway(
                "bKash",
                "bKash: You have received Tk 1,000.00. TrxID ABC1234567."
            ),
            None
        );
        assert_eq!(
            detect_in_pk_gateway("Nagad", "Nagad: payment of Tk 500 received."),
            None
        );
    }

    #[test]
    fn otp_messages_are_not_detected() {
        assert_eq!(
            detect_in_pk_gateway("Paytm", "Your OTP is 482913. Do not share it with anyone."),
            None
        );
        assert_eq!(
            detect_in_pk_gateway(
                "HDFCBK",
                "123456 is your one-time password for UPI registration. Do not share."
            ),
            None
        );
        assert_eq!(
            detect_in_pk_gateway(
                "VM-VERIFY",
                "Your verification code is 902104. It expires in 10 minutes."
            ),
            None
        );
    }

    #[test]
    fn unrelated_messages_are_not_detected() {
        assert_eq!(detect_in_pk_gateway("", ""), None);
        assert_eq!(
            detect_in_pk_gateway("AIRTEL", "Your data pack expires tomorrow. Recharge now!"),
            None
        );
        assert_eq!(
            detect_in_pk_gateway("HDFCBK", "Your account statement for last month is ready."),
            None
        );
    }

    #[test]
    fn global_gateways_are_not_detected_here() {
        assert_eq!(
            detect_in_pk_gateway("Stripe", "Payment of $20.00 received."),
            None
        );
        assert_eq!(
            detect_in_pk_gateway("PayPal", "You received a payment of $20.00."),
            None
        );
    }

    // ---- region_of -----------------------------------------------

    #[test]
    fn region_of_indian_gateways_is_in() {
        for gateway in [
            GatewayId::Upi,
            GatewayId::Paytm,
            GatewayId::Phonepe,
            GatewayId::Razorpay,
        ] {
            assert_eq!(region_of(gateway), Some("IN"), "{gateway:?}");
        }
    }

    #[test]
    fn region_of_pakistani_gateways_is_pk() {
        for gateway in [GatewayId::Easypaisa, GatewayId::Jazzcash] {
            assert_eq!(region_of(gateway), Some("PK"), "{gateway:?}");
        }
    }

    #[test]
    fn region_of_other_gateways_is_none() {
        for gateway in [
            GatewayId::Bkash,
            GatewayId::Nagad,
            GatewayId::Rocket,
            GatewayId::Upay,
            GatewayId::Surecash,
            GatewayId::Stripe,
            GatewayId::Paypal,
            GatewayId::Manual,
        ] {
            assert_eq!(region_of(gateway), None, "{gateway:?}");
        }
    }

    #[test]
    fn region_of_agrees_with_detection() {
        let cases: &[(&str, &str, GatewayId)] = &[
            ("Paytm", "Rs.100 paid.", GatewayId::Paytm),
            ("Easypaisa", "PKR 100 received.", GatewayId::Easypaisa),
        ];
        for (sender, body, expected) in cases {
            let detected = detect_in_pk_gateway(sender, body);
            assert_eq!(detected, Some(*expected));
            assert_eq!(region_of(*expected), region_of(detected.unwrap()));
        }
    }

    // ---- in_pk_sender_ids ----------------------------------------

    #[test]
    fn sender_ids_cover_brands_and_banks() {
        let ids = in_pk_sender_ids();
        assert!(!ids.is_empty());
        for expected in ["Paytm", "PhonePe", "Razorpay", "Easypaisa", "JazzCash", "HDFCBK"] {
            assert!(ids.contains(&expected), "missing sender id {expected}");
        }
        // No Bangladesh sender belongs in the IN/PK list.
        assert!(!ids.contains(&"bKash"));
        assert!(!ids.contains(&"Nagad"));
    }
}
