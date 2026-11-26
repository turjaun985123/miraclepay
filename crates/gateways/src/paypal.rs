//! PayPal gateway for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! PayPal is an *API-style* global gateway, unlike the manual
//! mobile-money gateways in this crate: there are no send-money
//! instructions and no merchant phone number. A configured adapter
//! holds the merchant's PayPal API credentials — the client id in
//! the config's API key field and the client secret in its API
//! secret field — and, once the provider integration is wired up,
//! would create an order through the PayPal Orders API and send the
//! customer to PayPal's hosted checkout. Verification likewise
//! belongs to PayPal: the real implementation captures / verifies
//! the order with PayPal before anything is marked verified.
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

/// Payment gateway for PayPal (API / hosted checkout flow).
///
/// Holds an optional [`GatewayConfig`]; a gateway built with
/// [`PaypalGateway::new_unconfigured`] (or with a disabled /
/// incomplete config) reports itself as not configured and refuses
/// to initiate or verify payments. Unlike the manual gateways,
/// configuration here means API credentials: the config must be
/// enabled and carry both a non-empty API key (the PayPal client
/// id) and a non-empty API secret (the PayPal client secret).
pub struct PaypalGateway {
    config: Option<GatewayConfig>,
}

impl PaypalGateway {
    /// A gateway with no configuration. Every operation that needs a
    /// configuration fails with [`GatewayError::NotConfigured`].
    pub fn new_unconfigured() -> Self {
        PaypalGateway { config: None }
    }

    /// A gateway using the given configuration.
    pub fn with_config(config: GatewayConfig) -> Self {
        PaypalGateway {
            config: Some(config),
        }
    }

    /// Whether the stored config is complete: a config is present,
    /// it is enabled, and it carries both a non-empty API key (the
    /// client id) and a non-empty API secret (the client secret).
    /// Either credential on its own is not enough — PayPal's OAuth
    /// client-credentials flow authenticates API calls with the pair.
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

impl Default for PaypalGateway {
    fn default() -> Self {
        Self::new_unconfigured()
    }
}

#[async_trait]
impl PaymentGateway for PaypalGateway {
    fn id(&self) -> GatewayId {
        GatewayId::Paypal
    }

    fn display_name(&self) -> &str {
        "PayPal"
    }

    fn is_configured(&self) -> bool {
        self.has_credentials()
    }

    async fn initiate(&self, req: &PaymentRequest) -> Result<InitiatedPayment, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("paypal".into()));
        }

        Ok(InitiatedPayment {
            provider_ref: req.reference.clone(),
            // Placeholder checkout URL: this only points at PayPal's
            // checkout with the caller's reference attached as a
            // token. A real payment requires creating an order
            // through the PayPal Orders API — authenticated
            // server-side with an OAuth client-credentials access
            // token obtained from the merchant's client id / client
            // secret — then handing the returned PayPal order id to
            // the checkout. No order is created here, so no money
            // can move from this URL alone.
            redirect_url: Some(format!(
                "https://www.paypal.com/checkoutnow?token={}",
                req.reference
            )),
            instructions: None,
            expires_at: None,
        })
    }

    async fn verify(&self, provider_ref: &str) -> Result<VerifyResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("paypal".into()));
        }

        // Nothing is verified here yet: the real implementation
        // captures / verifies the order with the PayPal Orders API —
        // fetching (and, where required, capturing) the order named
        // by `provider_ref` and only reporting `verified: true` when
        // PayPal confirms it as completed for the expected amount.
        // Until that lands, this always returns an unverified result
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
            return Err(GatewayError::NotConfigured("paypal".into()));
        }

        // PayPal does expose a refunds API, but it is not wired
        // into this adapter yet, so a configured gateway reports the
        // operation as unsupported rather than pretending a refund
        // was issued.
        Err(GatewayError::Unsupported(
            "paypal refund API not wired".into(),
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
            gateway: GatewayId::Paypal,
            merchant_number: None,
            api_key: api_key.map(str::to_string),
            api_secret: api_secret.map(str::to_string),
            extra: serde_json::Value::Null,
            sandbox: false,
            enabled,
        }
    }

    fn configured_gateway() -> PaypalGateway {
        PaypalGateway::with_config(config(
            true,
            Some("paypal_test_client_id"),
            Some("paypal_test_client_secret"),
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
        let gateway = PaypalGateway::new_unconfigured();
        assert_eq!(gateway.id(), GatewayId::Paypal);
        assert_eq!(gateway.display_name(), "PayPal");
        assert!(!gateway.is_configured());
    }

    #[test]
    fn api_key_only_is_not_configured() {
        // The client id without the client secret cannot authenticate
        // API calls.
        let key_only =
            PaypalGateway::with_config(config(true, Some("paypal_test_client_id"), None));
        assert!(!key_only.is_configured());

        // Symmetrically, the secret without the client id is
        // incomplete.
        let secret_only = PaypalGateway::with_config(config(
            true,
            None,
            Some("paypal_test_client_secret"),
        ));
        assert!(!secret_only.is_configured());

        // A complete pair on a disabled config is still not usable.
        let disabled = PaypalGateway::with_config(config(
            false,
            Some("paypal_test_client_id"),
            Some("paypal_test_client_secret"),
        ));
        assert!(!disabled.is_configured());

        assert!(configured_gateway().is_configured());
    }

    #[tokio::test]
    async fn unconfigured_initiate_is_not_configured_error() {
        let gateway = PaypalGateway::new_unconfigured();
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("unconfigured gateway must not initiate a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "paypal"),
            "expected NotConfigured(\"paypal\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn unconfigured_verify_is_not_configured_error() {
        let gateway = PaypalGateway::new_unconfigured();
        let err = gateway
            .verify("MP-TEST-001")
            .await
            .expect_err("unconfigured gateway must not verify a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "paypal"),
            "expected NotConfigured(\"paypal\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn configured_initiate_returns_redirect_url_containing_reference() {
        let gateway = configured_gateway();

        let initiated = gateway
            .initiate(&sample_request())
            .await
            .expect("configured gateway should initiate a payment");

        assert_eq!(initiated.provider_ref, "MP-TEST-001");
        assert_eq!(initiated.instructions, None);
        assert!(initiated.expires_at.is_none());

        let redirect_url = initiated
            .redirect_url
            .expect("API gateway must return a redirect URL");
        assert!(
            redirect_url.contains("MP-TEST-001"),
            "redirect URL must contain the payment reference, got: {redirect_url}"
        );
        assert!(
            redirect_url.starts_with("https://www.paypal.com/"),
            "redirect URL must point at PayPal checkout, got: {redirect_url}"
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

        let unconfigured = PaypalGateway::new_unconfigured();
        let err = unconfigured
            .refund(&refund)
            .await
            .expect_err("unconfigured gateway must not refund");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "paypal"),
            "expected NotConfigured(\"paypal\"), got: {err:?}"
        );

        let gateway = configured_gateway();
        let err = gateway
            .refund(&refund)
            .await
            .expect_err("paypal refund API is not wired yet");
        assert!(
            matches!(&err, GatewayError::Unsupported(msg) if msg == "paypal refund API not wired"),
            "expected Unsupported(\"paypal refund API not wired\"), got: {err:?}"
        );
    }
}
