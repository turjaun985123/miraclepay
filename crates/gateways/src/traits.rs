//! The payment-gateway plugin contract for MiraclePay v0.0.1.
//!
//! # Plugin architecture
//!
//! MiraclePay talks to every payment provider through exactly one
//! trait — [`PaymentGateway`] — and exactly one adapter per gateway.
//! An adapter is a small, self-contained type that owns one
//! provider's configuration (merchant number, API credentials, base
//! URL, and so on) and translates between MiraclePay's shared
//! request/response types in [`crate::types`] and that provider's
//! own way of taking and confirming a payment.
//!
//! Adding a gateway never means editing a central payment routine.
//! It means writing one new adapter type, implementing this trait
//! for it, and registering an instance of it alongside the others.
//! The rest of the platform — checkout, transaction recording,
//! webhooks, and settlement — only ever sees this trait, so it
//! cannot tell (and does not need to tell) one provider from
//! another beyond the [`crate::types::GatewayId`] the adapter
//! reports.
//!
//! Each operation is individually callable. A caller holding a
//! `&dyn PaymentGateway` (or a concrete adapter) can invoke
//! [`PaymentGateway::initiate`] on its own to start a payment,
//! [`PaymentGateway::verify`] on its own to ask the provider
//! whether a previously started payment actually completed, or
//! [`PaymentGateway::refund`] on its own where the provider
//! supports it. No operation is a hidden step inside another one:
//! starting, confirming, and returning a payment are separate
//! calls with separate inputs and outputs, so each can be retried,
//! scheduled, audited, or exposed as its own API endpoint without
//! dragging the others along.
//!
//! Two broad adapter families sit behind this one contract:
//!
//! * *Hosted/API gateways*, which redirect the payer to the
//!   provider or call the provider's API, and whose `verify` asks
//!   the provider directly about a provider reference.
//! * *Manual / SMS-verified gateways*, common for mobile wallets
//!   where the payer sends money by hand to a merchant number.
//!   Their `initiate` typically returns instructions built with
//!   [`manual_instructions`], and their `verify` is satisfied
//!   later — outside this trait — once an incoming transfer
//!   notification matching the payment's reference and amount has
//!   been observed.
//!
//! The trait is object-safe in practice: it is used through
//! `Box<dyn PaymentGateway>` (via the `async_trait` desugaring of
//! its async methods), so adapters of different concrete types can
//! be stored in one registry and selected at runtime by id.

use crate::types::GatewayError;

/// One payment gateway, implemented once per provider.
///
/// Implementors must be `Send + Sync` so a single adapter instance
/// can be shared across concurrent requests. Implementations are
/// expected to be cheap to query: [`PaymentGateway::id`],
/// [`PaymentGateway::display_name`], and
/// [`PaymentGateway::is_configured`] perform no network I/O.
#[async_trait::async_trait]
pub trait PaymentGateway: Send + Sync {
    /// The stable identifier of this gateway, unique across all
    /// registered adapters and safe to persist with a transaction.
    fn id(&self) -> crate::types::GatewayId;

    /// The human-readable name shown to merchants and payers,
    /// e.g. on a checkout page's list of payment options.
    fn display_name(&self) -> &str;

    /// Whether this adapter has everything it needs to operate
    /// (credentials present, merchant number set, and so on).
    ///
    /// An unconfigured gateway should still be listed — so a
    /// merchant can see it exists and finish setting it up — but
    /// must not be offered as a way to pay, and its `initiate`
    /// should fail with an error rather than start a payment that
    /// can never complete.
    fn is_configured(&self) -> bool;

    /// Start a payment of the amount and currency in `req`.
    ///
    /// On success this returns whatever the payer needs next —
    /// typically a redirect target, a provider reference, or
    /// manual send-money instructions — wrapped in an
    /// [`crate::types::InitiatedPayment`]. Starting a payment does
    /// not by itself mean any money has moved; that is established
    /// by [`PaymentGateway::verify`].
    async fn initiate(
        &self,
        req: &crate::types::PaymentRequest,
    ) -> Result<crate::types::InitiatedPayment, crate::types::GatewayError>;

    /// Ask the provider about the payment identified by
    /// `provider_ref`, a reference previously returned by
    /// [`PaymentGateway::initiate`] (or supplied by the provider
    /// in a callback), and report its current state.
    ///
    /// Implementations should treat an unknown or malformed
    /// reference as an error, not as a failed payment: "we cannot
    /// find this payment" and "this payment did not succeed" are
    /// different facts and callers rely on the distinction.
    async fn verify(
        &self,
        provider_ref: &str,
    ) -> Result<crate::types::VerifyResult, crate::types::GatewayError>;

    /// Return some or all of a completed payment to the payer.
    ///
    /// Refunds are not universal — in particular, manual /
    /// SMS-verified gateways have no provider mechanism to reverse
    /// a transfer the payer sent by hand — so the default
    /// implementation reports the operation as unsupported.
    /// Adapters whose provider offers refunds override this
    /// method; adapters that do not simply inherit the default,
    /// and callers must handle that error as a normal outcome
    /// rather than a malfunction.
    async fn refund(
        &self,
        _req: &crate::types::RefundRequest,
    ) -> Result<crate::types::RefundResult, crate::types::GatewayError> {
        Err(GatewayError::Unsupported(
            "refunds not supported by this gateway".to_string(),
        ))
    }
}

/// Build the payer-facing send-money instructions used by manual /
/// SMS-verified gateways.
///
/// The payer is told, in plain steps, to send exactly
/// `amount_display` (an already-formatted amount, including its
/// currency, e.g. `"BDT 500.00"`) to `number` using `gateway_name`,
/// and to put `reference` in the transfer's reference / note field.
/// That reference is what later lets an observed incoming transfer
/// be matched back to this payment, so the instructions stress
/// copying it exactly and keeping the provider's confirmation
/// message until the payment is marked complete.
///
/// No provider-specific wording is assumed: the same text serves
/// any wallet whose "send money" flow takes a number, an amount,
/// and a reference.
pub fn manual_instructions(
    gateway_name: &str,
    number: &str,
    amount_display: &str,
    reference: &str,
) -> String {
    format!(
        "Pay with {gateway_name} (send money):\n\
         1. Open your {gateway_name} app, or dial the {gateway_name} menu on your phone.\n\
         2. Choose the \"Send Money\" option.\n\
         3. Enter this number: {number}\n\
         4. Enter this exact amount: {amount_display}\n\
         5. In the reference / note field, enter this code exactly as shown: {reference}\n\
         6. Check the number, amount, and reference once more, then confirm the transfer.\n\
         7. Keep the confirmation message you receive — your payment is complete once the \
            transfer with reference {reference} has been received and matched.\n\
         Do not send a different amount and do not change the reference code: either one \
         will prevent your payment from being matched automatically."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal in-memory adapter used only to prove, at compile
    /// time, that the trait can be implemented as documented and
    /// used through a trait object. It performs no I/O and never
    /// reports a successful payment.
    struct DummyGateway;

    #[async_trait::async_trait]
    impl PaymentGateway for DummyGateway {
        fn id(&self) -> crate::types::GatewayId {
            crate::types::GatewayId::Manual
        }

        fn display_name(&self) -> &str {
            "Dummy"
        }

        fn is_configured(&self) -> bool {
            false
        }

        async fn initiate(
            &self,
            _req: &crate::types::PaymentRequest,
        ) -> Result<crate::types::InitiatedPayment, crate::types::GatewayError> {
            Err(GatewayError::Unsupported(
                "dummy gateway cannot initiate payments".to_string(),
            ))
        }

        async fn verify(
            &self,
            _provider_ref: &str,
        ) -> Result<crate::types::VerifyResult, crate::types::GatewayError> {
            Err(GatewayError::Unsupported(
                "dummy gateway cannot verify payments".to_string(),
            ))
        }

        // `refund` is deliberately not overridden: the dummy uses
        // the trait's default, which reports refunds as
        // unsupported.
    }

    #[test]
    fn gateway_can_be_boxed_as_a_trait_object() {
        let gateway: Box<dyn PaymentGateway> = Box::new(DummyGateway);
        assert_eq!(gateway.id(), crate::types::GatewayId::Manual);
        assert_eq!(gateway.display_name(), "Dummy");
        assert!(!gateway.is_configured());
    }

    #[test]
    fn manual_instructions_carry_every_matching_detail() {
        let text = manual_instructions("ExampleWallet", "01700000000", "BDT 500.00", "MP-TEST-1");
        assert!(text.contains("ExampleWallet"));
        assert!(text.contains("01700000000"));
        assert!(text.contains("BDT 500.00"));
        assert!(text.contains("MP-TEST-1"));
    }
}
