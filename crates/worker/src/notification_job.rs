//! Notification dispatch job for MiraclePay v0.0.1.
//!
//! This module is the worker-side half of outbound notifications. The
//! domain crate ([`miraclepay_domain::notification`]) owns the
//! [`Notification`] record and its body templates; this module owns
//! *delivery*: walking a queue of pending notifications, handing each
//! one to a transport, and recording the outcome back onto the record.
//!
//! Delivery itself is abstracted behind [`NotificationTransport`], a
//! deliberately tiny boxed-future trait. Real transports (an SMTP
//! client, an SMS provider, an HTTP webhook caller, …) live outside
//! this module and are plugged in by the worker binary; tests plug in
//! an in-memory mock instead. The boxed-future shape keeps the trait
//! object-safe and async without pulling in any extra dependency —
//! only `std` (`Pin` / `Future`) and `miraclepay-domain` are used here.

use std::future::Future;
use std::pin::Pin;

use miraclepay_domain::notification::{
    payment_received_body, Notification, NotificationChannel, NotificationStatus,
};

/// A delivery backend for notifications.
///
/// Implementors deliver a single message over the given
/// [`NotificationChannel`] and report success or a human-readable
/// failure reason. Implementations must be `Send + Sync` so one
/// transport can be shared across worker tasks.
///
/// The returned future borrows the transport and the message fields,
/// so implementations may hold those references for the duration of
/// the send without cloning them first.
pub trait NotificationTransport: Send + Sync {
    /// Send one message.
    ///
    /// * `channel` — the channel the message travels over.
    /// * `recipient` — where it goes on that channel: an email
    ///   address, a phone number, a callback URL, or an in-app key,
    ///   matching the channel.
    /// * `subject` — the subject line when the channel has one
    ///   (email); `None` for channels without a subject.
    /// * `body` — the plain-text message body.
    ///
    /// Returns `Ok(())` when the provider accepted the message and
    /// `Err(reason)` when delivery was attempted and failed.
    fn send<'a>(
        &'a self,
        channel: NotificationChannel,
        recipient: &'a str,
        subject: Option<&'a str>,
        body: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}

/// Outcome tallies for one [`dispatch_batch`] run.
///
/// Only notifications that were actually attempted are counted, so
/// `sent + failed` is the number of previously-queued notifications
/// in the batch; notifications that were already in a terminal state
/// are skipped and counted in neither field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DispatchSummary {
    /// Notifications the transport accepted; now `Sent`.
    pub sent: u32,
    /// Notifications the transport rejected; now `Failed`.
    pub failed: u32,
}

/// Dispatch every queued notification in `queue` through `transport`.
///
/// Notifications are processed in queue order. For each one whose
/// status is [`NotificationStatus::Queued`]:
///
/// * the transport is asked to send it with the notification's own
///   channel, recipient, subject, and body;
/// * on `Ok(())` the notification's status becomes
///   [`NotificationStatus::Sent`] and the summary's `sent` count
///   goes up by one;
/// * on `Err(_)` the notification's status becomes
///   [`NotificationStatus::Failed`] and the summary's `failed` count
///   goes up by one. The error text is not stored on the record —
///   the domain [`Notification`] has no field for it — so a failed
///   notification carries only its `Failed` status.
///
/// Notifications in any other status are left completely untouched:
/// the transport is never called for them and they are not counted
/// in the summary. A failure for one notification does not stop the
/// batch; the remaining queued notifications are still attempted.
pub async fn dispatch_batch(
    transport: &dyn NotificationTransport,
    queue: &mut Vec<Notification>,
) -> DispatchSummary {
    let mut summary = DispatchSummary::default();

    for notification in queue.iter_mut() {
        if notification.status != NotificationStatus::Queued {
            continue;
        }

        let result = transport
            .send(
                notification.channel,
                &notification.recipient,
                notification.subject.as_deref(),
                &notification.body,
            )
            .await;

        match result {
            Ok(()) => {
                notification.status = NotificationStatus::Sent;
                summary.sent += 1;
            }
            Err(_) => {
                notification.status = NotificationStatus::Failed;
                summary.failed += 1;
            }
        }
    }

    summary
}

/// Build the email notification sent when a payment completes.
///
/// The result is a queued [`NotificationChannel::Email`] notification
/// addressed to the customer, owned by `merchant_id`, with:
///
/// * a fresh id minted by `miraclepay_domain::ids::new_id("ntf")`,
/// * subject `"Payment received"`,
/// * body from the domain template
///   [`payment_received_body`], built from `amount_display` (the
///   amount exactly as the reader should see it, for example
///   `"BDT 1,500.00"`) and `trx_id` (the transaction reference the
///   reader can quote).
///
/// Nothing is sent here; hand the returned notification to
/// [`dispatch_batch`] (directly or via a persisted queue) to deliver
/// it.
pub fn payment_completed_notification(
    merchant_id: &str,
    customer_email: &str,
    amount_display: &str,
    trx_id: &str,
) -> Notification {
    Notification::new(
        miraclepay_domain::ids::new_id("ntf"),
        Some(merchant_id.to_string()),
        NotificationChannel::Email,
        customer_email,
        Some("Payment received".to_string()),
        payment_received_body(amount_display, trx_id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ---- minimal std-only executor -------------------------------------
    //
    // The mock transport's futures are ready immediately, so a tiny
    // poll loop with a no-op waker is enough to drive `dispatch_batch`
    // in tests without adding an async-runtime dependency here.

    struct NoopWaker;

    impl std::task::Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = std::task::Waker::from(Arc::new(NoopWaker));
        let mut context = std::task::Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(output) => return output,
                std::task::Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    // ---- mock transport --------------------------------------------------

    /// In-memory transport: succeeds for every recipient except the
    /// one named at construction, and records every recipient it was
    /// asked to send to so tests can prove skipped notifications were
    /// never attempted.
    struct MockTransport {
        failing_recipient: String,
        attempted: Mutex<Vec<String>>,
    }

    impl MockTransport {
        fn new(failing_recipient: &str) -> Self {
            Self {
                failing_recipient: failing_recipient.to_string(),
                attempted: Mutex::new(Vec::new()),
            }
        }

        fn attempted(&self) -> Vec<String> {
            self.attempted.lock().unwrap().clone()
        }
    }

    impl NotificationTransport for MockTransport {
        fn send<'a>(
            &'a self,
            _channel: NotificationChannel,
            recipient: &'a str,
            _subject: Option<&'a str>,
            _body: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
            self.attempted
                .lock()
                .unwrap()
                .push(recipient.to_string());
            let should_fail = recipient == self.failing_recipient.as_str();
            Box::pin(async move {
                if should_fail {
                    Err(format!("mock delivery failure for {recipient}"))
                } else {
                    Ok(())
                }
            })
        }
    }

    // ---- helpers -----------------------------------------------------------

    fn queued(id: &str, recipient: &str) -> Notification {
        Notification::new(
            id,
            Some("mch_1".to_string()),
            NotificationChannel::Email,
            recipient,
            Some("Payment received".to_string()),
            payment_received_body("BDT 1,500.00", "TRX-9001"),
        )
    }

    fn with_status(id: &str, recipient: &str, status: NotificationStatus) -> Notification {
        let mut notification = queued(id, recipient);
        notification.status = status;
        notification
    }

    // ---- dispatch_batch ----------------------------------------------------

    #[test]
    fn batch_counts_sent_and_failed() {
        let transport = MockTransport::new("bad@example.com");
        let mut queue = vec![
            queued("ntf_a", "good@example.com"),
            queued("ntf_b", "bad@example.com"),
            queued("ntf_c", "also-good@example.com"),
        ];

        let summary = block_on(dispatch_batch(&transport, &mut queue));

        assert_eq!(summary, DispatchSummary { sent: 2, failed: 1 });
    }

    #[test]
    fn batch_updates_statuses_from_transport_result() {
        let transport = MockTransport::new("bad@example.com");
        let mut queue = vec![
            queued("ntf_a", "good@example.com"),
            queued("ntf_b", "bad@example.com"),
        ];

        block_on(dispatch_batch(&transport, &mut queue));

        assert_eq!(queue[0].status, NotificationStatus::Sent);
        assert_eq!(queue[1].status, NotificationStatus::Failed);
    }

    #[test]
    fn batch_continues_after_a_failure() {
        let transport = MockTransport::new("bad@example.com");
        let mut queue = vec![
            queued("ntf_a", "bad@example.com"),
            queued("ntf_b", "good@example.com"),
        ];

        let summary = block_on(dispatch_batch(&transport, &mut queue));

        assert_eq!(summary, DispatchSummary { sent: 1, failed: 1 });
        assert_eq!(queue[1].status, NotificationStatus::Sent);
        // Both queued notifications were attempted, in order.
        assert_eq!(
            transport.attempted(),
            vec![
                "bad@example.com".to_string(),
                "good@example.com".to_string()
            ]
        );
    }

    #[test]
    fn batch_skips_non_queued_notifications() {
        let transport = MockTransport::new("bad@example.com");
        let mut queue = vec![
            with_status("ntf_a", "done@example.com", NotificationStatus::Sent),
            with_status("ntf_b", "gone@example.com", NotificationStatus::Failed),
            queued("ntf_c", "good@example.com"),
        ];

        let summary = block_on(dispatch_batch(&transport, &mut queue));

        // Only the queued one is counted or attempted.
        assert_eq!(summary, DispatchSummary { sent: 1, failed: 0 });
        assert_eq!(
            transport.attempted(),
            vec!["good@example.com".to_string()]
        );
        // Non-queued records are untouched.
        assert_eq!(queue[0].status, NotificationStatus::Sent);
        assert_eq!(queue[1].status, NotificationStatus::Failed);
        assert_eq!(queue[2].status, NotificationStatus::Sent);
    }

    #[test]
    fn batch_over_empty_queue_counts_nothing() {
        let transport = MockTransport::new("bad@example.com");
        let mut queue: Vec<Notification> = Vec::new();

        let summary = block_on(dispatch_batch(&transport, &mut queue));

        assert_eq!(summary, DispatchSummary { sent: 0, failed: 0 });
        assert!(transport.attempted().is_empty());
    }

    // ---- payment_completed_notification ------------------------------------

    #[test]
    fn builder_sets_all_fields() {
        let n = payment_completed_notification(
            "mch_42",
            "customer@example.com",
            "BDT 1,500.00",
            "TRX-9001",
        );

        assert!(n.id.starts_with("ntf_"), "id was: {}", n.id);
        assert_eq!(n.merchant_id, Some("mch_42".to_string()));
        assert_eq!(n.channel, NotificationChannel::Email);
        assert_eq!(n.recipient, "customer@example.com");
        assert_eq!(n.subject, Some("Payment received".to_string()));
        assert_eq!(n.status, NotificationStatus::Queued);
        assert_eq!(n.body, payment_received_body("BDT 1,500.00", "TRX-9001"));
        assert!(n.body.contains("BDT 1,500.00"), "body was: {}", n.body);
        assert!(n.body.contains("TRX-9001"), "body was: {}", n.body);
    }

    #[test]
    fn builder_mints_a_unique_id_per_call() {
        let a = payment_completed_notification("mch_1", "a@example.com", "BDT 1.00", "TRX-1");
        let b = payment_completed_notification("mch_1", "a@example.com", "BDT 1.00", "TRX-1");
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn built_notification_dispatches_through_the_batch() {
        let transport = MockTransport::new("bad@example.com");
        let mut queue = vec![payment_completed_notification(
            "mch_42",
            "customer@example.com",
            "BDT 2,300.00",
            "TRX-777",
        )];

        let summary = block_on(dispatch_batch(&transport, &mut queue));

        assert_eq!(summary, DispatchSummary { sent: 1, failed: 0 });
        assert_eq!(queue[0].status, NotificationStatus::Sent);
    }
}
