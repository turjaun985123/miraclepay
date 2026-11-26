//! SMS corpus regression tests (Wave 31).
//!
//! A fixed corpus of realistic payment SMS messages across every
//! supported region (BD / IN / PK), plus negative cases that must
//! never settle a payment. All names, numbers, and ids are
//! fictional fixtures. If a gateway changes its SMS wording, this
//! corpus is where the breakage shows up first.
//!
//! Expectations are the *plain meaning* of each message. A parser
//! change that alters a result here needs a deliberate corpus
//! update in the same commit — never a silent one.

use std::collections::HashSet;

use chrono::{TimeZone, Utc};
use miraclepay_domain::money::{Currency, Money};
use miraclepay_domain::transaction::{Transaction, TxStatus};
use miraclepay_gateways::types::GatewayId;
use miraclepay_sms_verify::types::IncomingSms;
use miraclepay_sms_verify::{parse_incoming, verify_sms, VerificationOutcome};

fn sms(sender: &str, body: &str) -> IncomingSms {
    IncomingSms {
        device_id: "device-1".to_string(),
        sender: sender.to_string(),
        body: body.to_string(),
        received_at: Utc.with_ymd_and_hms(2026, 11, 26, 4, 0, 0).unwrap(),
        sim_slot: None,
    }
}

struct Case {
    name: &'static str,
    sender: &'static str,
    body: &'static str,
    gateway: Option<GatewayId>,
    amount_minor: Option<i64>,
    currency: Option<Currency>,
    trx_id: Option<&'static str>,
    is_payment: bool,
}

const CASES: &[Case] = &[
    // ---------------- Bangladesh ----------------
    Case {
        name: "bkash received",
        sender: "bKash",
        body: "You have received Tk 1,250.00 from 01712345678. TrxID: 9HX7K2LMNP. Balance: Tk 5,430.50. Thank you for using bKash.",
        gateway: Some(GatewayId::Bkash),
        amount_minor: Some(125_000),
        currency: Some(Currency::BDT),
        trx_id: Some("9HX7K2LMNP"),
        is_payment: true,
    },
    Case {
        name: "bkash cash-in",
        sender: "bKash",
        body: "Cash In Tk 500.00 from 01912345678 successful. TrxID: AKJ72HSD81. Balance: Tk 1,020.00.",
        gateway: Some(GatewayId::Bkash),
        amount_minor: Some(50_000),
        currency: Some(Currency::BDT),
        trx_id: Some("AKJ72HSD81"),
        is_payment: true,
    },
    Case {
        name: "nagad received",
        sender: "Nagad",
        body: "You have received Tk 1,250.00 from 01712-345678. Trx ID: NAG7X2K9P1. Your new balance is Tk 5,430.50. - Nagad",
        gateway: Some(GatewayId::Nagad),
        amount_minor: Some(125_000),
        currency: Some(Currency::BDT),
        trx_id: Some("NAG7X2K9P1"),
        is_payment: true,
    },
    Case {
        name: "rocket received",
        sender: "Rocket",
        body: "Tk 850.00 received from 01712-345678 to your Rocket account 01798-765432. Txn ID: 3829104756. Balance: Tk 2,140.00. - DBBL Rocket",
        gateway: Some(GatewayId::Rocket),
        amount_minor: Some(85_000),
        currency: Some(Currency::BDT),
        trx_id: Some("3829104756"),
        is_payment: true,
    },
    Case {
        name: "upay received",
        sender: "Upay",
        body: "You have received Tk. 2,000.00 from 01812-345678. TrxID: UPY88XZ21K. New Balance: Tk. 9,999.00 - Upay",
        gateway: Some(GatewayId::Upay),
        amount_minor: Some(200_000),
        currency: Some(Currency::BDT),
        trx_id: Some("UPY88XZ21K"),
        is_payment: true,
    },
    Case {
        name: "surecash received",
        sender: "SureCash",
        body: "You have received Tk 300.00 from 01612345678. TrxID: SRC55TQ901. Balance: Tk 800.00. - SureCash",
        gateway: Some(GatewayId::Surecash),
        amount_minor: Some(30_000),
        currency: Some(Currency::BDT),
        trx_id: Some("SRC55TQ901"),
        is_payment: true,
    },
    // ---------------- India ----------------
    Case {
        name: "paytm received",
        sender: "Paytm",
        body: "Payment of Rs.250 received in your Paytm wallet. Txn ID: 202311234567.",
        gateway: Some(GatewayId::Paytm),
        amount_minor: Some(25_000),
        currency: Some(Currency::INR),
        trx_id: Some("202311234567"),
        is_payment: true,
    },
    Case {
        name: "phonepe credit",
        sender: "PhonePe",
        body: "Rs.199 credited to your account via PhonePe. UTR 123456789012.",
        gateway: Some(GatewayId::Phonepe),
        amount_minor: Some(19_900),
        currency: Some(Currency::INR),
        trx_id: Some("123456789012"),
        is_payment: true,
    },
    Case {
        name: "upi credit",
        sender: "VM-UPIBNK",
        body: "Rs.1,000.00 credited to your account via UPI. UPI Ref 123456789012.",
        gateway: Some(GatewayId::Upi),
        amount_minor: Some(100_000),
        currency: Some(Currency::INR),
        trx_id: Some("123456789012"),
        is_payment: true,
    },
    // ---------------- Pakistan ----------------
    Case {
        name: "easypaisa received",
        sender: "Easypaisa",
        body: "You have received PKR 5,000 in your Easypaisa account. Trx ID 123456789.",
        gateway: Some(GatewayId::Easypaisa),
        amount_minor: Some(500_000),
        currency: Some(Currency::PKR),
        trx_id: Some("123456789"),
        is_payment: true,
    },
    Case {
        name: "jazzcash received",
        sender: "JazzCash",
        body: "You have received PKR 3,000 in your JazzCash account. TID 654321.",
        gateway: Some(GatewayId::Jazzcash),
        amount_minor: Some(300_000),
        currency: Some(Currency::PKR),
        trx_id: Some("654321"),
        is_payment: true,
    },
    // ---------------- Negatives ----------------
    Case {
        name: "otp is not a payment",
        sender: "bKash",
        body: "Your bKash OTP is 482913. Do not share it with anyone.",
        gateway: None,
        amount_minor: None,
        currency: None,
        trx_id: None,
        is_payment: false,
    },
    Case {
        name: "promo is not a payment",
        sender: "Nagad",
        body: "Nagad offer! Get 10% cashback up to Tk 100 on your next payment. Dial *167# to know more.",
        gateway: None,
        amount_minor: None,
        currency: None,
        trx_id: None,
        is_payment: false,
    },
];

#[test]
fn corpus_parses_as_expected() {
    for case in CASES {
        let parsed = parse_incoming(&sms(case.sender, case.body));
        if !case.is_payment {
            // Non-payments either error (NotAPaymentSms) or parse to
            // a non-payment record — both are acceptable, settling
            // is not (covered by the verify test below).
            if let Ok(parsed) = &parsed {
                assert!(
                    !parsed.is_payment(),
                    "case '{}': parsed as a payment: {parsed:?}",
                    case.name
                );
            }
            continue;
        }
        let parsed = parsed.unwrap_or_else(|e| panic!("case '{}': parse error {e:?}", case.name));
        assert_eq!(parsed.gateway, case.gateway, "case '{}': gateway", case.name);
        assert_eq!(
            parsed.amount_minor, case.amount_minor,
            "case '{}': amount",
            case.name
        );
        assert_eq!(
            parsed.currency, case.currency,
            "case '{}': currency",
            case.name
        );
        assert_eq!(
            parsed.trx_id.as_deref(),
            case.trx_id,
            "case '{}': trx id",
            case.name
        );
        assert!(parsed.is_payment(), "case '{}': must be a payment", case.name);
    }
}

#[test]
fn failed_payment_notice_never_verifies() {
    // Wave 31 regression: this message carries an amount AND a
    // TrxID and matches a pending payment on both — before the
    // failure-notice guard it would have settled the transaction.
    let body = "Your bKash payment of Tk 1,250.00 from 01712345678 has FAILED. \
                TrxID: 9HX7K2LMNP. Please try again.";
    let now = Utc.with_ymd_and_hms(2026, 11, 26, 3, 55, 0).unwrap();
    let tx = Transaction {
        id: "tx-fail-1".to_string(),
        merchant_id: "merchant-1".to_string(),
        gateway: "bkash".to_string(),
        amount: Money::new(125_000, Currency::BDT),
        status: TxStatus::Pending,
        customer_email: None,
        customer_phone: None,
        provider_trx_id: Some("9HX7K2LMNP".to_string()),
        sender_number: Some("01712345678".to_string()),
        reference: None,
        metadata: serde_json::Value::Null,
        created_at: now,
        updated_at: now,
    };
    let outcome = verify_sms(&sms("bKash", body), &[tx], &HashSet::new());
    assert_eq!(outcome, VerificationOutcome::NotPayment);
    assert!(parse_incoming(&sms("bKash", body)).is_err());
}

#[test]
fn insufficient_balance_notice_never_verifies() {
    let body = "bKash: Payment of Tk 250.00 unsuccessful due to insufficient balance. TrxID: INS1234567.";
    let outcome = verify_sms(&sms("bKash", body), &[], &HashSet::new());
    assert_eq!(outcome, VerificationOutcome::NotPayment);
}
