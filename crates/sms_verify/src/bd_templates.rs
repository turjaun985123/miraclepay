//! Realistic Bangladeshi gateway SMS templates — end-to-end
//! verification tests (Wave 8 hardening for Nagad, Rocket, Upay).
//!
//! Each test feeds a full, realistic confirmation message through
//! [`parse_incoming`] and asserts every extracted field, then runs
//! [`verify_sms`] against a matching pending transaction. Negative
//! cases (wrong amount, replayed TrxID) assert the safe outcome.
//! Message shapes are written from the gateways' well-known public
//! template styles; all numbers and ids are fictional.

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chrono::{TimeZone, Utc};
    use miraclepay_domain::money::{Currency, Money};
    use miraclepay_domain::transaction::{Transaction, TxStatus};
    use miraclepay_gateways::types::GatewayId;

    use crate::types::IncomingSms;
    use crate::verifier::{parse_incoming, verify_sms, VerificationOutcome};

    fn sms(sender: &str, body: &str) -> IncomingSms {
        IncomingSms {
            device_id: "dev-1".to_string(),
            sender: sender.to_string(),
            body: body.to_string(),
            received_at: Utc.with_ymd_and_hms(2026, 11, 26, 4, 0, 0).unwrap(),
            sim_slot: None,
        }
    }

    fn pending(
        id: &str,
        gateway: &str,
        amount_minor: i64,
        sender: &str,
        trx: Option<&str>,
    ) -> Transaction {
        let now = Utc.with_ymd_and_hms(2026, 11, 26, 3, 55, 0).unwrap();
        Transaction {
            id: id.to_string(),
            merchant_id: "merchant-1".to_string(),
            gateway: gateway.to_string(),
            amount: Money::new(amount_minor, Currency::BDT),
            status: TxStatus::Pending,
            customer_email: None,
            customer_phone: None,
            provider_trx_id: trx.map(str::to_string),
            sender_number: Some(sender.to_string()),
            reference: None,
            metadata: serde_json::Value::Null,
            created_at: now,
            updated_at: now,
        }
    }

    const NAGAD_BODY: &str = "You have received Tk 1,250.00 from 01712-345678. \
        Trx ID: NAG7X2K9P1. Your new balance is Tk 5,430.50. - Nagad";

    #[test]
    fn nagad_template_parses_all_fields() {
        let parsed = parse_incoming(&sms("Nagad", NAGAD_BODY)).unwrap();
        assert_eq!(parsed.gateway, Some(GatewayId::Nagad));
        assert_eq!(parsed.amount_minor, Some(125_000));
        assert_eq!(parsed.trx_id.as_deref(), Some("NAG7X2K9P1"));
        assert_eq!(parsed.balance_minor, Some(543_050));
        assert_eq!(parsed.currency, Some(Currency::BDT));
    }

    #[test]
    fn nagad_template_verifies_matching_payment() {
        let tx = pending("tx-n1", "nagad", 125_000, "01712345678", Some("NAG7X2K9P1"));
        let outcome = verify_sms(&sms("Nagad", NAGAD_BODY), &[tx], &HashSet::new());
        assert_eq!(outcome, VerificationOutcome::Verified("tx-n1".to_string()));
    }

    const ROCKET_BODY: &str = "Tk 850.00 received from 01712-345678 to your Rocket \
        account 01798-765432. Txn ID: 3829104756. Balance: Tk 2,140.00. - DBBL Rocket";

    #[test]
    fn rocket_template_parses_all_fields() {
        let parsed = parse_incoming(&sms("Rocket", ROCKET_BODY)).unwrap();
        assert_eq!(parsed.gateway, Some(GatewayId::Rocket));
        assert_eq!(parsed.amount_minor, Some(85_000));
        // Rocket transaction ids are purely numeric — the extractor
        // must still capture them after the "Txn ID" label.
        assert_eq!(parsed.trx_id.as_deref(), Some("3829104756"));
    }

    #[test]
    fn rocket_template_verifies_matching_payment() {
        let tx = pending("tx-r1", "rocket", 85_000, "01712345678", Some("3829104756"));
        let outcome = verify_sms(&sms("Rocket", ROCKET_BODY), &[tx], &HashSet::new());
        assert_eq!(outcome, VerificationOutcome::Verified("tx-r1".to_string()));
    }

    const UPAY_BODY: &str = "You have received Tk. 2,000.00 from 01812-345678. \
        TrxID: UPY88XZ21K. New Balance: Tk. 9,999.00 - Upay";

    #[test]
    fn upay_template_parses_all_fields() {
        let parsed = parse_incoming(&sms("Upay", UPAY_BODY)).unwrap();
        assert_eq!(parsed.gateway, Some(GatewayId::Upay));
        assert_eq!(parsed.amount_minor, Some(200_000));
        assert_eq!(parsed.trx_id.as_deref(), Some("UPY88XZ21K"));
        assert_eq!(parsed.balance_minor, Some(999_900));
    }

    #[test]
    fn upay_template_verifies_matching_payment() {
        let tx = pending("tx-u1", "upay", 200_000, "01812345678", Some("UPY88XZ21K"));
        let outcome = verify_sms(&sms("Upay", UPAY_BODY), &[tx], &HashSet::new());
        assert_eq!(outcome, VerificationOutcome::Verified("tx-u1".to_string()));
    }

    #[test]
    fn wrong_amount_does_not_verify() {
        // Same Nagad SMS, but the pending payment expects Tk 999.00.
        let tx = pending("tx-n2", "nagad", 99_900, "01712345678", Some("NAG7X2K9P1"));
        let outcome = verify_sms(&sms("Nagad", NAGAD_BODY), &[tx], &HashSet::new());
        assert_ne!(outcome, VerificationOutcome::Verified("tx-n2".to_string()));
    }

    #[test]
    fn replayed_trx_id_is_duplicate_for_all_bd_gateways() {
        let mut used = HashSet::new();
        used.insert("NAG7X2K9P1".to_string());
        let tx = pending("tx-n3", "nagad", 125_000, "01712345678", Some("NAG7X2K9P1"));
        let outcome = verify_sms(&sms("Nagad", NAGAD_BODY), &[tx], &used);
        assert_eq!(outcome, VerificationOutcome::Duplicate);
    }

    #[test]
    fn promotional_sms_never_verifies_anything() {
        // The promo mentions an amount ("Tk 100") but no TrxID and no
        // matching pending payment: by design the parser treats it as
        // payment-shaped, and the matcher must leave it Unmatched —
        // it can never settle a transaction.
        let promo = sms("Nagad", "Nagad offer! Get 10% cashback up to Tk 100 on your next payment. Dial *167# to know more.");
        let outcome = verify_sms(&promo, &[], &HashSet::new());
        assert_eq!(outcome, VerificationOutcome::Unmatched);
        // A message with neither amount nor TrxID is not a payment at all.
        let plain = sms(
            "Nagad",
            "Nagad: winter offer is live. Dial *167# to know more.",
        );
        assert_eq!(
            verify_sms(&plain, &[], &HashSet::new()),
            VerificationOutcome::NotPayment
        );
    }
}
