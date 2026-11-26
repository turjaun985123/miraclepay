//! Stripe gateway for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! Stripe is an *API-style* global card gateway, unlike the manual
//! mobile-money gateways in this crate: there are no send-money
//! instructions and no merchant phone number. A configured adapter
//! holds the merchant's Stripe publishable key / secret key pair
//! and, once the provider integration is wired up, would create a
//! Checkout Session through the Stripe API and send the customer
//! to Stripe's hosted checkout. Verification likewise belongs to
//! Stripe: the real implementation retrieves the Checkout Session
//! from Stripe (or verifies the webhook signature on Stripe's
//! completion event) before anything is marked verified.
//!
//! In this version the adapter enforces its configuration contract
//! and returns a placeholder checkout URL; it never claims a payment
//! is verified, and refunds report themselves as not yet wired.

use async_trait::async_trait;

use crate::traits::PaymentGateway;
use crate::types::{
    GatewayConfig, GatewayError, GatewayId, InitiatedPayment, PaymentRequest, RefundRequest,
    RefundResult, VerifyResult,
};

/// Payment gateway for Stripe (API / hosted checkout flow).
///
/// Holds an optional [`GatewayConfig`]; a gateway built with
/// [`StripeGateway::new_unconfigured`] (or with a disabled /
/// incomplete config) reports itself as not configured and refuses
/// to initiate or verify payments. Unlike the manual gateways,
/// configuration here means API credentials: the config must be
/// enabled and carry both a non-empty API key (Stripe's publishable
/// key) and a non-empty API secret (Stripe's secret key).
pub struct StripeGateway {
    config: Option<GatewayConfig>,
}

impl StripeGateway {
    /// A gateway with no configuration. Every operation that needs a
    /// configuration fails with [`GatewayError::NotConfigured`].
    pub fn new_unconfigured() -> Self {
        StripeGateway { config: None }
    }

    /// A gateway using the given configuration.
    pub fn with_config(config: GatewayConfig) -> Self {
        StripeGateway {
            config: Some(config),
        }
    }

    /// Whether the stored config is complete: a config is present,
    /// it is enabled, and it carries both a non-empty API key
    /// (publishable key) and a non-empty API secret (secret key).
    /// Either credential on its own is not enough — the publishable
    /// key identifies the account to Stripe Checkout, and the secret
    /// key authenticates server-side API calls.
    fn has_credentials(&self) -> bool {
        let Some(config) = self.config.as_ref() else {
            return false;
        };
        if !config.enabled {
            return false;
        }
        let has_key = config
            .api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty());
        let has_secret = config
            .api_secret
            .as_deref()
            .is_some_and(|secret| !secret.trim().is_empty());
        has_key && has_secret
    }
}

impl Default for StripeGateway {
    fn default() -> Self {
        Self::new_unconfigured()
    }
}

#[async_trait]
impl PaymentGateway for StripeGateway {
    fn id(&self) -> GatewayId {
        GatewayId::Stripe
    }

    fn display_name(&self) -> &str {
        "Stripe"
    }

    fn is_configured(&self) -> bool {
        self.has_credentials()
    }

    async fn initiate(&self, _req: &PaymentRequest) -> Result<InitiatedPayment, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("stripe".into()));
        }

        // Wave 30: NO fabricated checkout URL (previously
        // `checkout.stripe.com/c/pay/<reference>`, a URL that could
        // never take a payment). A real payment needs a Checkout
        // Session created server-side; the request spec lives in
        // `stripe_checkout` (Wave 9, env-gated). Fail honestly until
        // the transport lands.
        Err(GatewayError::Provider(
            "stripe: create a Checkout Session via stripe_checkout::StripeCheckoutConfig \
             before redirecting; direct initiate is not supported"
                .into(),
        ))
    }

    async fn verify(&self, provider_ref: &str) -> Result<VerifyResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("stripe".into()));
        }

        // Nothing is verified here yet: the real implementation
        // retrieves the Checkout Session from the Stripe API and
        // checks its payment status, or verifies the webhook
        // signature on the `checkout.session.completed` event Stripe
        // sends when a payment completes, and only reports
        // `verified: true` when Stripe confirms the payment. Until
        // that lands, this always returns an unverified result
        // rather than claiming a confirmation it cannot back up.
        Ok(VerifyResult {
            provider_trx_id: provider_ref.to_string(),
            amount_minor: 0,
            sender: None,
            verified: false,
        })
    }

    async fn refund(&self, _req: &RefundRequest) -> Result<RefundResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("stripe".into()));
        }

        // Stripe does expose a refunds API, but it is not wired
        // into this adapter yet, so a configured gateway reports the
        // operation as unsupported rather than pretending a refund
        // was issued.
        Err(GatewayError::Unsupported(
            "stripe refund API not wired".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miraclepay_domain::{Currency, Money};

    fn config(
        enabled: bool,
        api_key: Option<&str>,
        api_secret: Option<&str>,
    ) -> GatewayConfig {
        GatewayConfig {
            gateway: GatewayId::Stripe,
            merchant_number: None,
            api_key: api_key.map(str::to_string),
            api_secret: api_secret.map(str::to_string),
            extra: serde_json::Value::Null,
            sandbox: false,
            enabled,
        }
    }

    fn configured_gateway() -> StripeGateway {
        StripeGateway::with_config(config(
            true,
            Some("pk_test_key"),
            Some("sk_test_secret"),
        ))
    }

    fn sample_request() -> PaymentRequest {
        PaymentRequest {
            merchant_id: "merchant-1".to_string(),
            amount: Money::new(50_000, Currency::USD),
            reference: "MP-TEST-001".to_string(),
            customer_name: None,
            customer_email: None,
            customer_phone: None,
            callback_url: None,
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn unconfigured_gateway_reports_not_configured() {
        let gateway = StripeGateway::new_unconfigured();
        assert_eq!(gateway.id(), GatewayId::Stripe);
        assert_eq!(gateway.display_name(), "Stripe");
        assert!(!gateway.is_configured());
    }

    #[test]
    fn api_key_only_is_not_configured() {
        // The publishable key without the secret key cannot
        // authenticate server-side API calls.
        let key_only = StripeGateway::with_config(config(true, Some("pk_test_key"), None));
        assert!(!key_only.is_configured());

        // Symmetrically, the secret without the key is incomplete.
        let secret_only =
            StripeGateway::with_config(config(true, None, Some("sk_test_secret")));
        assert!(!secret_only.is_configured());

        // A complete pair on a disabled config is still not usable.
        let disabled = StripeGateway::with_config(config(
            false,
            Some("pk_test_key"),
            Some("sk_test_secret"),
        ));
        assert!(!disabled.is_configured());

        assert!(configured_gateway().is_configured());
    }

    #[tokio::test]
    async fn unconfigured_initiate_is_not_configured_error() {
        let gateway = StripeGateway::new_unconfigured();
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("unconfigured gateway must not initiate a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "stripe"),
            "expected NotConfigured(\"stripe\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn unconfigured_verify_is_not_configured_error() {
        let gateway = StripeGateway::new_unconfigured();
        let err = gateway
            .verify("MP-TEST-001")
            .await
            .expect_err("unconfigured gateway must not verify a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "stripe"),
            "expected NotConfigured(\"stripe\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn configured_initiate_fails_honestly_without_session_transport() {
        // Wave 30: no fabricated Stripe checkout URL may be returned.
        let gateway = configured_gateway();
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("no fabricated Stripe checkout URL may be returned");
        assert!(
            matches!(&err, GatewayError::Provider(msg) if msg.contains("stripe_checkout")),
            "expected Provider(..stripe_checkout..), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn configured_verify_is_unverified() {
        let gateway = configured_gateway();
        let result = gateway
            .verify("MP-TEST-001")
            .await
            .expect("configured gateway should answer a verify call");
        assert_eq!(result.provider_trx_id, "MP-TEST-001");
        assert_eq!(result.amount_minor, 0);
        assert_eq!(result.sender, None);
        assert!(!result.verified);
    }

    #[tokio::test]
    async fn refund_requires_configuration_then_is_unsupported() {
        let refund = RefundRequest {
            provider_ref: "MP-TEST-001".to_string(),
            amount_minor: 50_000,
            reason: None,
        };

        let unconfigured = StripeGateway::new_unconfigured();
        let err = unconfigured
            .refund(&refund)
            .await
            .expect_err("unconfigured gateway must not refund");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "stripe"),
            "expected NotConfigured(\"stripe\"), got: {err:?}"
        );

        let gateway = configured_gateway();
        let err = gateway
            .refund(&refund)
            .await
            .expect_err("stripe refund API is not wired yet");
        assert!(
            matches!(&err, GatewayError::Unsupported(msg) if msg == "stripe refund API not wired"),
            "expected Unsupported(\"stripe refund API not wired\"), got: {err:?}"
        );
    }
}
