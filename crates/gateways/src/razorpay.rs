//! Razorpay gateway for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! Razorpay is an *API-style* Indian gateway, unlike the manual
//! mobile-money gateways in this crate: there are no send-money
//! instructions and no merchant phone number. A configured adapter
//! holds the merchant's Razorpay API key / secret pair and, once the
//! provider integration is wired up, would create an order through
//! the Razorpay Orders API and send the customer to Razorpay's
//! hosted checkout. Verification likewise belongs to Razorpay: the
//! real implementation checks the HMAC signature Razorpay returns
//! with a completed payment before anything is marked verified.
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

/// Payment gateway for Razorpay (API / hosted checkout flow).
///
/// Holds an optional [`GatewayConfig`]; a gateway built with
/// [`RazorpayGateway::new_unconfigured`] (or with a disabled /
/// incomplete config) reports itself as not configured and refuses
/// to initiate or verify payments. Unlike the manual gateways,
/// configuration here means API credentials: the config must be
/// enabled and carry both a non-empty API key and a non-empty API
/// secret.
pub struct RazorpayGateway {
    config: Option<GatewayConfig>,
}

impl RazorpayGateway {
    /// A gateway with no configuration. Every operation that needs a
    /// configuration fails with [`GatewayError::NotConfigured`].
    pub fn new_unconfigured() -> Self {
        RazorpayGateway { config: None }
    }

    /// A gateway using the given configuration.
    pub fn with_config(config: GatewayConfig) -> Self {
        RazorpayGateway {
            config: Some(config),
        }
    }

    /// Whether the stored config is complete: a config is present,
    /// it is enabled, and it carries both a non-empty API key and a
    /// non-empty API secret. Either credential on its own is not
    /// enough — Razorpay authenticates API calls with the pair.
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

impl Default for RazorpayGateway {
    fn default() -> Self {
        Self::new_unconfigured()
    }
}

#[async_trait]
impl PaymentGateway for RazorpayGateway {
    fn id(&self) -> GatewayId {
        GatewayId::Razorpay
    }

    fn display_name(&self) -> &str {
        "Razorpay"
    }

    fn is_configured(&self) -> bool {
        self.has_credentials()
    }

    async fn initiate(&self, req: &PaymentRequest) -> Result<InitiatedPayment, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("razorpay".into()));
        }
        if req.amount.currency != miraclepay_domain::Currency::INR {
            return Err(GatewayError::Provider(format!(
                "razorpay payments must be in INR, got {}",
                req.amount.currency.code()
            )));
        }

        // Wave 30: NO fabricated checkout URL. A real Razorpay
        // payment needs an Order created server-side first; the
        // request spec for that call lives in `razorpay_orders`
        // (env-gated credentials, transport lands with live keys).
        // Until the transport exists, fail honestly instead of
        // handing the customer a URL that cannot take a payment.
        Err(GatewayError::Provider(
            "razorpay: create an order via razorpay_orders::RazorpayConfig \
             (Orders API) before checkout; direct initiate is not supported"
                .into(),
        ))
    }

    async fn verify(&self, provider_ref: &str) -> Result<VerifyResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("razorpay".into()));
        }

        // Nothing is verified here yet: the real implementation
        // verifies the Razorpay signature — an HMAC of the Razorpay
        // order id and payment id, keyed with the merchant's API
        // secret — that Razorpay returns when a payment completes,
        // and only reports `verified: true` when that signature
        // checks out. Until that lands, this always returns an
        // unverified result rather than claiming a confirmation it
        // cannot back up.
        Ok(VerifyResult {
            provider_trx_id: provider_ref.to_string(),
            amount_minor: 0,
            sender: None,
            verified: false,
        })
    }

    async fn refund(&self, _req: &RefundRequest) -> Result<RefundResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("razorpay".into()));
        }

        // Razorpay does expose a refunds API, but it is not wired
        // into this adapter yet, so a configured gateway reports the
        // operation as unsupported rather than pretending a refund
        // was issued.
        Err(GatewayError::Unsupported(
            "razorpay refund API not wired".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miraclepay_domain::{Currency, Money};

    fn config(enabled: bool, api_key: Option<&str>, api_secret: Option<&str>) -> GatewayConfig {
        GatewayConfig {
            gateway: GatewayId::Razorpay,
            merchant_number: None,
            api_key: api_key.map(str::to_string),
            api_secret: api_secret.map(str::to_string),
            extra: serde_json::Value::Null,
            sandbox: false,
            enabled,
        }
    }

    fn configured_gateway() -> RazorpayGateway {
        RazorpayGateway::with_config(config(true, Some("rzp_test_key"), Some("rzp_test_secret")))
    }

    fn sample_request() -> PaymentRequest {
        PaymentRequest {
            merchant_id: "merchant-1".to_string(),
            amount: Money::new(50_000, Currency::INR),
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
        let gateway = RazorpayGateway::new_unconfigured();
        assert_eq!(gateway.id(), GatewayId::Razorpay);
        assert_eq!(gateway.display_name(), "Razorpay");
        assert!(!gateway.is_configured());
    }

    #[test]
    fn api_key_only_is_not_configured() {
        // The key without the secret cannot authenticate API calls.
        let key_only = RazorpayGateway::with_config(config(true, Some("rzp_test_key"), None));
        assert!(!key_only.is_configured());

        // Symmetrically, the secret without the key is incomplete.
        let secret_only = RazorpayGateway::with_config(config(true, None, Some("rzp_test_secret")));
        assert!(!secret_only.is_configured());

        // A complete pair on a disabled config is still not usable.
        let disabled = RazorpayGateway::with_config(config(
            false,
            Some("rzp_test_key"),
            Some("rzp_test_secret"),
        ));
        assert!(!disabled.is_configured());

        assert!(configured_gateway().is_configured());
    }

    #[tokio::test]
    async fn unconfigured_initiate_is_not_configured_error() {
        let gateway = RazorpayGateway::new_unconfigured();
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("unconfigured gateway must not initiate a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "razorpay"),
            "expected NotConfigured(\"razorpay\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn unconfigured_verify_is_not_configured_error() {
        let gateway = RazorpayGateway::new_unconfigured();
        let err = gateway
            .verify("MP-TEST-001")
            .await
            .expect_err("unconfigured gateway must not verify a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "razorpay"),
            "expected NotConfigured(\"razorpay\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn configured_initiate_fails_honestly_without_orders_transport() {
        // Wave 30: previously this returned a fabricated checkout
        // URL that could never take a payment. It must now error,
        // pointing at the Orders API spec module.
        let gateway = configured_gateway();
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("no fabricated Razorpay checkout URL may be returned");
        assert!(
            matches!(&err, GatewayError::Provider(msg) if msg.contains("razorpay_orders")),
            "expected Provider(..razorpay_orders..), got: {err:?}"
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

        let unconfigured = RazorpayGateway::new_unconfigured();
        let err = unconfigured
            .refund(&refund)
            .await
            .expect_err("unconfigured gateway must not refund");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "razorpay"),
            "expected NotConfigured(\"razorpay\"), got: {err:?}"
        );

        let gateway = configured_gateway();
        let err = gateway
            .refund(&refund)
            .await
            .expect_err("razorpay refund API is not wired yet");
        assert!(
            matches!(&err, GatewayError::Unsupported(msg) if msg == "razorpay refund API not wired"),
            "expected Unsupported(\"razorpay refund API not wired\"), got: {err:?}"
        );
    }
}
