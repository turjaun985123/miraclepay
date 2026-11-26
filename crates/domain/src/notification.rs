//! Notifications for MiraclePay.
//!
//! A [`Notification`] is a single message the platform sends to a merchant
//! or a customer about something that happened in their account: a payment
//! arrived, an invoice was issued, a settlement was paid out, and so on.
//! Each notification travels over exactly one [`NotificationChannel`] and
//! carries a plain-text body, most often produced by one of the small
//! template helpers in this module.
//!
//! The helpers only format text. They take already-formatted display
//! strings (amounts, transaction ids, invoice numbers) so this module
//! never has to know about currencies, locales, or money arithmetic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Channel a [`Notification`] is delivered over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    /// Sent to an email address.
    Email,
    /// Sent as a text message to a phone number.
    Sms,
    /// Sent as an HTTP callback to a merchant-provided URL.
    Webhook,
    /// Shown inside the merchant dashboard / app, with no external delivery.
    InApp,
}

/// Delivery state of a [`Notification`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationStatus {
    /// Created and waiting to be handed to the delivery provider.
    Queued,
    /// Accepted / delivered by the provider for the chosen channel.
    Sent,
    /// Delivery was attempted and failed.
    Failed,
}

impl NotificationStatus {
    /// Whether this is a final state that will not change on its own.
    ///
    /// `Sent` and `Failed` are terminal; a `Queued` notification can
    /// still move to either of them.
    pub fn is_terminal(self) -> bool {
        matches!(self, NotificationStatus::Sent | NotificationStatus::Failed)
    }
}

/// A single outbound message to a merchant or a customer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// Unique notification identifier.
    pub id: String,
    /// Merchant this notification belongs to, if any. Platform-wide
    /// messages (for example a maintenance notice) have no merchant.
    pub merchant_id: Option<String>,
    /// Channel this message is delivered over.
    pub channel: NotificationChannel,
    /// Where the message goes on that channel: an email address for
    /// [`NotificationChannel::Email`], a phone number for
    /// [`NotificationChannel::Sms`], a callback URL for
    /// [`NotificationChannel::Webhook`], or an in-app inbox key for
    /// [`NotificationChannel::InApp`].
    pub recipient: String,
    /// Subject line, when the channel has one (email). `None` for
    /// channels without a subject, such as SMS.
    pub subject: Option<String>,
    /// Plain-text message body, usually built by a template helper below.
    pub body: String,
    /// Current delivery state.
    pub status: NotificationStatus,
    /// When this notification record was created.
    pub created_at: DateTime<Utc>,
}

impl Notification {
    /// Create a new notification in [`NotificationStatus::Queued`],
    /// stamped with the current time.
    pub fn new(
        id: impl Into<String>,
        merchant_id: Option<String>,
        channel: NotificationChannel,
        recipient: impl Into<String>,
        subject: Option<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            merchant_id,
            channel,
            recipient: recipient.into(),
            subject,
            body: body.into(),
            status: NotificationStatus::Queued,
            created_at: Utc::now(),
        }
    }
}

/// Body text for a "payment received" message.
///
/// `amount_display` is the amount as the reader should see it (for
/// example `"BDT 1,500.00"`) and `trx_id` is the transaction reference
/// the reader can quote when asking about the payment.
pub fn payment_received_body(amount_display: &str, trx_id: &str) -> String {
    format!(
        "Good news - a payment of {amount_display} has been received. \
         Transaction reference: {trx_id}. \
         The amount will be included in your next settlement."
    )
}

/// Body text for an "invoice sent" message.
///
/// `number` is the human-readable invoice number (for example
/// `"INV-1042"`) and `total_display` is the invoice total as the reader
/// should see it (for example `"BDT 2,300.00"`).
pub fn invoice_sent_body(number: &str, total_display: &str) -> String {
    format!(
        "Invoice {number} for {total_display} has been sent to your customer. \
         You will be notified here as soon as it is paid."
    )
}

/// Body text for a "settlement paid" message.
///
/// `net_display` is the net payout amount as the reader should see it
/// (for example `"BDT 48,750.00"`), i.e. the settlement total after the
/// platform fee has been deducted.
pub fn settlement_paid_body(net_display: &str) -> String {
    format!(
        "Your settlement of {net_display} has been paid out. \
         Please allow some time for the funds to appear in your payout account."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification() -> Notification {
        Notification {
            id: "ntf_1".to_string(),
            merchant_id: Some("mch_1".to_string()),
            channel: NotificationChannel::Email,
            recipient: "merchant@example.com".to_string(),
            subject: Some("Payment received".to_string()),
            body: payment_received_body("BDT 1,500.00", "TRX-9001"),
            status: NotificationStatus::Queued,
            created_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        }
    }

    // ---- templates -----------------------------------------------------

    #[test]
    fn payment_received_body_includes_amount_and_reference() {
        let body = payment_received_body("BDT 1,500.00", "TRX-9001");
        assert!(body.contains("BDT 1,500.00"), "body was: {body}");
        assert!(body.contains("TRX-9001"), "body was: {body}");
    }

    #[test]
    fn payment_received_body_uses_the_given_values_verbatim() {
        let body = payment_received_body("$25.50", "abc-123");
        assert!(body.contains("$25.50"), "body was: {body}");
        assert!(body.contains("abc-123"), "body was: {body}");
        // Nothing from a different call leaks into this one.
        assert!(!body.contains("TRX-9001"), "body was: {body}");
    }

    #[test]
    fn invoice_sent_body_includes_number_and_total() {
        let body = invoice_sent_body("INV-1042", "BDT 2,300.00");
        assert!(body.contains("INV-1042"), "body was: {body}");
        assert!(body.contains("BDT 2,300.00"), "body was: {body}");
    }

    #[test]
    fn settlement_paid_body_includes_net_amount() {
        let body = settlement_paid_body("BDT 48,750.00");
        assert!(body.contains("BDT 48,750.00"), "body was: {body}");
    }

    #[test]
    fn template_bodies_are_non_empty_single_messages() {
        for body in [
            payment_received_body("BDT 1.00", "TRX-1"),
            invoice_sent_body("INV-1", "BDT 1.00"),
            settlement_paid_body("BDT 1.00"),
        ] {
            assert!(!body.trim().is_empty());
            assert!(!body.contains('\n'), "body was: {body}");
        }
    }

    // ---- Notification basics -------------------------------------------

    #[test]
    fn new_notification_starts_queued() {
        let n = Notification::new(
            "ntf_2",
            None,
            NotificationChannel::Sms,
            "+8801700000000",
            None,
            settlement_paid_body("BDT 10.00"),
        );
        assert_eq!(n.status, NotificationStatus::Queued);
        assert_eq!(n.id, "ntf_2");
        assert_eq!(n.merchant_id, None);
        assert_eq!(n.channel, NotificationChannel::Sms);
        assert_eq!(n.recipient, "+8801700000000");
        assert_eq!(n.subject, None);
    }

    #[test]
    fn terminal_statuses_are_sent_and_failed() {
        assert!(NotificationStatus::Sent.is_terminal());
        assert!(NotificationStatus::Failed.is_terminal());
        assert!(!NotificationStatus::Queued.is_terminal());
    }

    // ---- serde -----------------------------------------------------------

    #[test]
    fn channels_serialize_as_snake_case() {
        let cases = [
            (NotificationChannel::Email, "\"email\""),
            (NotificationChannel::Sms, "\"sms\""),
            (NotificationChannel::Webhook, "\"webhook\""),
            (NotificationChannel::InApp, "\"in_app\""),
        ];
        for (channel, expected) in cases {
            assert_eq!(serde_json::to_string(&channel).unwrap(), expected);
            assert_eq!(
                serde_json::from_str::<NotificationChannel>(expected).unwrap(),
                channel
            );
        }
    }

    #[test]
    fn statuses_serialize_as_snake_case() {
        let cases = [
            (NotificationStatus::Queued, "\"queued\""),
            (NotificationStatus::Sent, "\"sent\""),
            (NotificationStatus::Failed, "\"failed\""),
        ];
        for (status, expected) in cases {
            assert_eq!(serde_json::to_string(&status).unwrap(), expected);
            assert_eq!(
                serde_json::from_str::<NotificationStatus>(expected).unwrap(),
                status
            );
        }
    }

    #[test]
    fn notification_round_trips_through_json() {
        let original = notification();
        let json = serde_json::to_string(&original).unwrap();
        let restored: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn notification_json_uses_expected_field_names() {
        let json = serde_json::to_string(&notification()).unwrap();
        for field in [
            "\"id\"",
            "\"merchant_id\"",
            "\"channel\"",
            "\"recipient\"",
            "\"subject\"",
            "\"body\"",
            "\"status\"",
            "\"created_at\"",
        ] {
            assert!(json.contains(field), "missing {field} in: {json}");
        }
        assert!(json.contains("\"channel\":\"email\""), "json was: {json}");
        assert!(json.contains("\"status\":\"queued\""), "json was: {json}");
    }

    #[test]
    fn notification_without_merchant_or_subject_round_trips() {
        let mut n = notification();
        n.merchant_id = None;
        n.subject = None;
        n.channel = NotificationChannel::Webhook;
        let json = serde_json::to_string(&n).unwrap();
        assert!(json.contains("\"merchant_id\":null"), "json was: {json}");
        assert!(json.contains("\"subject\":null"), "json was: {json}");
        let restored: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, n);
    }
}
