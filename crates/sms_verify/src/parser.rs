//! Generic SMS field extraction for MiraclePay v0.0.1.
//!
//! This module is deliberately provider-agnostic: it knows nothing about
//! any particular wallet, bank, or gateway message template. It looks for
//! the *shapes* that transactional SMS messages share across Bangladesh,
//! India, and Pakistan:
//!
//! * an amount written next to a currency marker (`Tk`, `BDT`, `৳`, `Rs`,
//!   `INR`, `PKR`) or after an `Amount` label,
//! * a transaction / reference identifier after a label such as `TrxID`,
//!   `Transaction ID`, `Ref ID`, or `UTR`,
//! * a mobile number in a Bangladeshi, Indian, or Pakistani format, and
//! * a remaining balance after a `Balance` label.
//!
//! All amounts are returned in *minor* units (poisha / paise): the value
//! `Tk 1,250.00` is returned as `125000`. Returning integers avoids any
//! floating-point rounding in money handling.
//!
//! Every regular expression is compiled exactly once, lazily, behind a
//! [`std::sync::OnceLock`], so calling these functions in a hot loop over
//! many messages pays no repeated compilation cost.
//!
//! Dependencies: `regex` and the Rust standard library only.

use std::sync::OnceLock;

use regex::Regex;

// ---------------------------------------------------------------------------
// Lazily compiled patterns
// ---------------------------------------------------------------------------

/// A bare numeric amount: comma-grouped thousands, optional 1–2 decimals.
///
/// Examples: `500`, `1,250`, `1,250.00`, `75.5`.
const NUM: &str = r"[0-9][0-9,]*(?:\.[0-9]{1,2})?";

/// Currency markers recognised next to an amount (case-insensitive at the
/// call site). `Tk`/`BDT`/`৳` are Bangladeshi taka, `Rs`/`INR` are Indian
/// rupees, and `Rs`/`PKR` are Pakistani rupees.
const CURRENCY: &str = r"(?:Tk\.?|BDT|৳|Rs\.?|INR|PKR)";

/// Amount pattern with three capture alternatives:
///
/// 1. an `Amount` / `Amt` label, optionally followed by a currency marker,
/// 2. a currency marker before the number (`Tk 1,250.00`, `Tk1250`),
/// 3. a currency marker after the number (`1,250.00 Tk`).
fn amount_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let pattern = format!(
            r"(?i)(?:amount|amt)\s*[:\-]?\s*{currency}?\s*(?P<label>{num})|{currency}\s*(?P<prefix>{num})|(?P<suffix>{num})\s*{currency}",
            currency = CURRENCY,
            num = NUM,
        );
        Regex::new(&pattern).expect("amount pattern is a valid regex")
    })
}

/// Balance pattern: a `Balance` label (optionally `Current Balance`,
/// `Available Balance`, `New Balance`, …), an optional colon / `is`, an
/// optional currency marker, then the amount.
fn balance_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let pattern = format!(
            r"(?i)balance\s*[:\-]?\s*(?:is\s+)?{currency}?\s*(?P<bal>{num})",
            currency = CURRENCY,
            num = NUM,
        );
        Regex::new(&pattern).expect("balance pattern is a valid regex")
    })
}

/// Transaction-id pattern: a known label, an optional `No` / `Number`, an
/// optional separator (`:`, `#`, `-`, `.`), then 6–24 alphanumeric chars.
fn trx_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:trx\s*id|transaction\s*id|trans\s*id|txn\s*id|ref(?:erence)?\s*id|utr)(?:\s*(?:no|number)\.?)?\s*[:#\-.]?\s*([A-Za-z0-9]{6,24})",
        )
        .expect("trx id pattern is a valid regex")
    })
}

/// Phone pattern. The first capture group is the number itself; the match
/// is anchored by a non-digit (or string edge) on both sides so a shorter
/// alternative can never match *inside* a longer digit run.
///
/// Alternatives, longest / most specific first:
/// * `+8801XXXXXXXXX` — Bangladesh, international form,
/// * `+91XXXXXXXXXX` — India, international form,
/// * `8801XXXXXXXXX` — Bangladesh, country code without `+`,
/// * `01XXXXXXXXX` — Bangladesh, local form (operator prefix `013`–`019`),
/// * `03XXXXXXXXX` — Pakistan, local form,
/// * `XXXXXXXXXX` — India, 10-digit mobile starting `6`–`9`.
fn phone_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?:^|[^0-9])(\+8801[3-9][0-9]{8}|\+91[6-9][0-9]{9}|8801[3-9][0-9]{8}|01[3-9][0-9]{8}|03[0-9]{9}|[6-9][0-9]{9})(?:[^0-9]|$)",
        )
        .expect("phone pattern is a valid regex")
    })
}

// ---------------------------------------------------------------------------
// Amount string parsing
// ---------------------------------------------------------------------------

/// Parse a human-written amount string into minor units.
///
/// Strips comma thousands separators and surrounding whitespace, then
/// converts the decimal value exactly (no floating point):
///
/// * `"500"` → `Some(50000)`
/// * `"1,250.00"` → `Some(125000)`
/// * `"75.5"` → `Some(7550)`
/// * `"0.05"` → `Some(5)`
///
/// Returns `None` for empty, negative, non-numeric, over-precise (more
/// than two decimal places), or overflowing input.
pub fn parse_amount_str(s: &str) -> Option<i64> {
    let cleaned: String = s
        .trim()
        .chars()
        .filter(|c| *c != ',' && !c.is_whitespace())
        .collect();
    if cleaned.is_empty() || cleaned.starts_with('-') || cleaned.starts_with('+') {
        return None;
    }

    let (major_str, minor) = match cleaned.split_once('.') {
        Some((major, frac)) => {
            if frac.is_empty() || frac.len() > 2 || !frac.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            // One decimal digit means tenths: ".5" is 50 minor units.
            let mut value: i64 = frac.parse().ok()?;
            if frac.len() == 1 {
                value *= 10;
            }
            (major, value)
        }
        None => (cleaned.as_str(), 0),
    };

    if major_str.is_empty() || !major_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let major: i64 = major_str.parse().ok()?;
    major.checked_mul(100)?.checked_add(minor)
}

// ---------------------------------------------------------------------------
// Field extraction
// ---------------------------------------------------------------------------

/// Whether the text immediately before `start` mentions a balance, in
/// which case an amount found there is a balance, not the transacted
/// amount. Looks back over a short window so phrasing such as
/// `"Balance Tk 2,300.00"` is recognised.
fn preceded_by_balance(body: &str, start: usize) -> bool {
    let window_start = start.saturating_sub(24);
    // `window_start` is a byte index derived from another match boundary
    // minus a constant; step back to a char boundary to stay UTF-8 safe.
    let mut idx = window_start;
    while idx > 0 && !body.is_char_boundary(idx) {
        idx -= 1;
    }
    body[idx..start].to_ascii_lowercase().contains("bal")
}

/// Extract the transacted amount, in minor units, from an SMS body.
///
/// Finds the first amount written next to a currency marker (`Tk`, `BDT`,
/// `৳`, `Rs`, `INR`, `PKR`) or after an `Amount` label. Amounts that are
/// clearly labelled as a *balance* are skipped, so in
/// `"Balance Tk 9,999.00. You received Tk 250.00"` this returns `25000`;
/// use [`extract_balance_minor`] for the balance itself.
///
/// * `"You have received Tk 1,250.00"` → `Some(125000)`
/// * `"Tk1250 debited"` → `Some(125000)`
/// * `"Amount: Rs 750"` → `Some(75000)`
/// * `"Hello, no money here"` → `None`
pub fn extract_amount_minor(body: &str) -> Option<i64> {
    for caps in amount_re().captures_iter(body) {
        // Skip candidates that are the balance, not the transaction.
        let whole = caps.get(0)?;
        if preceded_by_balance(body, whole.start()) {
            continue;
        }
        let text = caps
            .name("label")
            .or_else(|| caps.name("prefix"))
            .or_else(|| caps.name("suffix"))?
            .as_str();
        if let Some(value) = parse_amount_str(text) {
            return Some(value);
        }
    }
    None
}

/// Extract the remaining balance, in minor units, from an SMS body.
///
/// Looks for an amount following a `Balance` label:
///
/// * `"Balance Tk 2,300.00"` → `Some(230000)`
/// * `"Current Balance: Rs 1,000.50"` → `Some(100050)`
/// * `"No balance mentioned"` → `None`
pub fn extract_balance_minor(body: &str) -> Option<i64> {
    let caps = balance_re().captures(body)?;
    parse_amount_str(caps.name("bal")?.as_str())
}

/// Extract a transaction / reference identifier from an SMS body.
///
/// Recognises the labels `TrxID`, `Trans ID`, `Transaction ID`, `Txn ID`,
/// `Ref ID`, `Reference ID`, and `UTR` (case-insensitive), with an
/// optional separator, followed by 6–24 alphanumeric characters. The
/// identifier is returned exactly as written, without any trailing
/// punctuation.
///
/// * `"TrxID 9HX7K2LMN1."` → `Some("9HX7K2LMN1")`
/// * `"UTR: 402918273645"` → `Some("402918273645")`
/// * `"no reference here"` → `None`
pub fn extract_trx_id(body: &str) -> Option<String> {
    let caps = trx_id_re().captures(body)?;
    Some(caps.get(1)?.as_str().to_string())
}

/// Extract the first plausible sender / counterparty mobile number.
///
/// Recognises Bangladeshi (`01XXXXXXXXX`, `8801…`, `+8801…`), Pakistani
/// (`03XXXXXXXXX`), and Indian (`XXXXXXXXXX`, `+91…`) mobile formats and
/// returns the number exactly as written in the message.
///
/// * `"... from 01712345678. ..."` → `Some("01712345678")`
/// * `"Sender +919876543210 paid you"` → `Some("+919876543210")`
/// * `"no phone in this text"` → `None`
pub fn extract_phone(body: &str) -> Option<String> {
    let caps = phone_re().captures(body)?;
    Some(caps.get(1)?.as_str().to_string())
}

// ---------------------------------------------------------------------------
// Tests — all sample messages below are original, invented for this module.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical invented sample used across several tests.
    const SAMPLE: &str = "You have received Tk 500.00 from 01712345678. \
                          TrxID 9HX7K2LMN1. Balance Tk 2,300.00";

    // -- parse_amount_str ---------------------------------------------------

    #[test]
    fn parse_plain_integer() {
        assert_eq!(parse_amount_str("500"), Some(50000));
    }

    #[test]
    fn parse_with_commas_and_decimals() {
        assert_eq!(parse_amount_str("1,250.00"), Some(125000));
    }

    #[test]
    fn parse_large_grouped_amount() {
        assert_eq!(parse_amount_str("12,34,567.89"), Some(123456789));
    }

    #[test]
    fn parse_single_decimal_digit_is_tenths() {
        assert_eq!(parse_amount_str("75.5"), Some(7550));
    }

    #[test]
    fn parse_small_fraction_only_value() {
        assert_eq!(parse_amount_str("0.05"), Some(5));
    }

    #[test]
    fn parse_trims_surrounding_whitespace() {
        assert_eq!(parse_amount_str("  2,300.00  "), Some(230000));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert_eq!(parse_amount_str(""), None);
        assert_eq!(parse_amount_str("abc"), None);
        assert_eq!(parse_amount_str("12a.00"), None);
        assert_eq!(parse_amount_str("1.234"), None); // too precise
        assert_eq!(parse_amount_str("-50.00"), None); // negative
        assert_eq!(parse_amount_str("."), None);
    }

    #[test]
    fn parse_rejects_overflow() {
        assert_eq!(parse_amount_str("99999999999999999999.00"), None);
    }

    // -- extract_amount_minor ------------------------------------------------

    #[test]
    fn amount_from_canonical_sample() {
        assert_eq!(extract_amount_minor(SAMPLE), Some(50000));
    }

    #[test]
    fn amount_tk_with_commas_and_decimals() {
        assert_eq!(
            extract_amount_minor("Cash in Tk 1,250.00 successful."),
            Some(125000)
        );
    }

    #[test]
    fn amount_tk_without_space() {
        assert_eq!(extract_amount_minor("Tk1250 debited from your account."), Some(125000));
    }

    #[test]
    fn amount_bdt_prefix_and_suffix() {
        assert_eq!(extract_amount_minor("Payment of BDT 3,000 received."), Some(300000));
        assert_eq!(extract_amount_minor("Payment of 3,000 BDT received."), Some(300000));
    }

    #[test]
    fn amount_taka_sign() {
        assert_eq!(extract_amount_minor("You paid ৳850.50 at the shop."), Some(85050));
    }

    #[test]
    fn amount_indian_rupees() {
        assert_eq!(
            extract_amount_minor("Rs 2,499.00 credited to your account."),
            Some(249900)
        );
        assert_eq!(extract_amount_minor("INR 999 received via UPI."), Some(99900));
    }

    #[test]
    fn amount_pakistani_rupees() {
        assert_eq!(
            extract_amount_minor("PKR 5,000.00 has been transferred."),
            Some(500000)
        );
    }

    #[test]
    fn amount_after_amount_label() {
        assert_eq!(extract_amount_minor("Amount: 750.25 confirmed."), Some(75025));
        assert_eq!(extract_amount_minor("Transfer Amount Tk 420 done."), Some(42000));
    }

    #[test]
    fn amount_skips_balance_that_comes_first() {
        let body = "Balance Tk 9,999.00. You received Tk 250.00 just now.";
        assert_eq!(extract_amount_minor(body), Some(25000));
    }

    #[test]
    fn amount_is_none_without_currency_context() {
        assert_eq!(extract_amount_minor("Your OTP is 482913. Do not share it."), None);
        assert_eq!(extract_amount_minor("Hello, welcome to our service."), None);
        assert_eq!(extract_amount_minor(""), None);
    }

    // -- extract_balance_minor ------------------------------------------------

    #[test]
    fn balance_from_canonical_sample() {
        assert_eq!(extract_balance_minor(SAMPLE), Some(230000));
    }

    #[test]
    fn balance_with_colon_and_current_label() {
        assert_eq!(
            extract_balance_minor("Current Balance: Rs 1,000.50 only."),
            Some(100050)
        );
    }

    #[test]
    fn balance_without_currency_marker() {
        assert_eq!(extract_balance_minor("Your new balance is 640.00."), Some(64000));
    }

    #[test]
    fn balance_is_none_when_absent() {
        assert_eq!(extract_balance_minor("You received Tk 100.00 today."), None);
        assert_eq!(extract_balance_minor(""), None);
    }

    // -- extract_trx_id --------------------------------------------------------

    #[test]
    fn trx_from_canonical_sample() {
        assert_eq!(extract_trx_id(SAMPLE), Some("9HX7K2LMN1".to_string()));
    }

    #[test]
    fn trx_label_variants() {
        assert_eq!(
            extract_trx_id("Trans ID: BK77XZ21QW done."),
            Some("BK77XZ21QW".to_string())
        );
        assert_eq!(
            extract_trx_id("Transaction ID - 8f3Kd92LmZp7 confirmed."),
            Some("8f3Kd92LmZp7".to_string())
        );
        assert_eq!(
            extract_trx_id("Ref ID #MNP4502918 for your records."),
            Some("MNP4502918".to_string())
        );
        assert_eq!(
            extract_trx_id("Payment done. UTR 402918273645."),
            Some("402918273645".to_string())
        );
    }

    #[test]
    fn trx_is_case_insensitive_for_label() {
        assert_eq!(
            extract_trx_id("trxid ab12cd34ef received."),
            Some("ab12cd34ef".to_string())
        );
    }

    #[test]
    fn trx_excludes_trailing_punctuation() {
        assert_eq!(
            extract_trx_id("TrxID: ZX81QW99AA."),
            Some("ZX81QW99AA".to_string())
        );
    }

    #[test]
    fn trx_too_short_is_none() {
        assert_eq!(extract_trx_id("TrxID AB12."), None);
        assert_eq!(extract_trx_id("No reference in this message."), None);
        assert_eq!(extract_trx_id(""), None);
    }

    // -- extract_phone ----------------------------------------------------------

    #[test]
    fn phone_from_canonical_sample() {
        assert_eq!(extract_phone(SAMPLE), Some("01712345678".to_string()));
    }

    #[test]
    fn phone_bd_international_forms() {
        assert_eq!(
            extract_phone("Received from +8801812345678 today."),
            Some("+8801812345678".to_string())
        );
        assert_eq!(
            extract_phone("Received from 8801912345678 today."),
            Some("8801912345678".to_string())
        );
    }

    #[test]
    fn phone_bd_operator_prefixes() {
        assert_eq!(
            extract_phone("Call 01398765432 for details."),
            Some("01398765432".to_string())
        );
        assert_eq!(
            extract_phone("Call 01911223344 for details."),
            Some("01911223344".to_string())
        );
    }

    #[test]
    fn phone_pakistani_mobile() {
        assert_eq!(
            extract_phone("Sent by 03001234567 via mobile wallet."),
            Some("03001234567".to_string())
        );
    }

    #[test]
    fn phone_indian_mobile() {
        assert_eq!(
            extract_phone("Sender 9876543210 paid you."),
            Some("9876543210".to_string())
        );
        assert_eq!(
            extract_phone("Sender +919876543210 paid you."),
            Some("+919876543210".to_string())
        );
    }

    #[test]
    fn phone_returns_first_plausible_number() {
        let body = "Tk 200.00 from 01611112222 to 01733334444.";
        assert_eq!(extract_phone(body), Some("01611112222".to_string()));
    }

    #[test]
    fn phone_is_none_without_a_number() {
        assert_eq!(extract_phone("Your payment was successful."), None);
        assert_eq!(extract_phone("Dial 16247 for support."), None);
        assert_eq!(extract_phone(""), None);
    }

    // -- full-message integration ------------------------------------------------

    #[test]
    fn full_bd_message_all_fields() {
        assert_eq!(extract_amount_minor(SAMPLE), Some(50000));
        assert_eq!(extract_trx_id(SAMPLE), Some("9HX7K2LMN1".to_string()));
        assert_eq!(extract_phone(SAMPLE), Some("01712345678".to_string()));
        assert_eq!(extract_balance_minor(SAMPLE), Some(230000));
    }

    #[test]
    fn full_indian_message_all_fields() {
        let body = "Rs 1,499.00 received from 9812345670. \
                    UTR: 319876543210. Balance Rs 8,020.75";
        assert_eq!(extract_amount_minor(body), Some(149900));
        assert_eq!(extract_trx_id(body), Some("319876543210".to_string()));
        assert_eq!(extract_phone(body), Some("9812345670".to_string()));
        assert_eq!(extract_balance_minor(body), Some(802075));
    }

    #[test]
    fn full_pakistani_message_all_fields() {
        let body = "PKR 12,000 received from 03219876543. \
                    Ref ID PKTX88Q21Z. Balance PKR 45,500.00";
        assert_eq!(extract_amount_minor(body), Some(1200000));
        assert_eq!(extract_trx_id(body), Some("PKTX88Q21Z".to_string()));
        assert_eq!(extract_phone(body), Some("03219876543".to_string()));
        assert_eq!(extract_balance_minor(body), Some(4550000));
    }

    #[test]
    fn non_transactional_message_yields_nothing() {
        let body = "Dear customer, our branch will be closed on Friday.";
        assert_eq!(extract_amount_minor(body), None);
        assert_eq!(extract_trx_id(body), None);
        assert_eq!(extract_phone(body), None);
        assert_eq!(extract_balance_minor(body), None);
    }
}
