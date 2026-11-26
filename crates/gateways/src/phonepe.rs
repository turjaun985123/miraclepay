//! PhonePe gateway for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! PhonePe is a *manual* Indian gateway in this version: there is no
//! hosted checkout redirect. Initiating a payment returns instructions
//! telling the customer to send money to the merchant's PhonePe number
//! and to quote the payment reference. The payment is only ever marked
//! verified later, by the SMS verification engine matching an incoming
//! PhonePe SMS against the pending payment — never by this module.
//! Refunds are likewise handled manually by the merchant, outside the
//! gateway.

use async_trait::async_trait;

use crate::traits::PaymentGateway;
use crate::types::{
    GatewayConfig, GatewayError, GatewayId, InitiatedPayment, PaymentRequest, RefundRequest,
    RefundResult, VerifyResult,
};

/// Payment gateway for PhonePe (manual send-money flow).
///
/// Holds an optional [`GatewayConfig`]; a gateway built with
/// [`PhonepeGateway::new_unconfigured`] (or with a disabled / incomplete
/// config) reports itself as not configured and refuses to initiate or
/// verify payments.
pub struct PhonepeGateway {
    config: Option<GatewayConfig>,
}

impl PhonepeGateway {
    /// A gateway with no configuration. Every operation that needs a
    /// configuration fails with [`GatewayError::NotConfigured`].
    pub fn new_unconfigured() -> Self {
        PhonepeGateway { config: None }
    }

    /// A gateway using the given configuration.
    pub fn with_config(config: GatewayConfig) -> Self {
        PhonepeGateway {
            config: Some(config),
        }
    }

    /// The configured merchant PhonePe number, when the gateway is fully
    /// configured: a config is present, it is enabled, and it carries a
    /// non-empty merchant number.
    fn merchant_number(&self) -> Option<&str> {
        // Wave 30: Indian mobile or VPA required (see `in_manual`).
        crate::in_manual::merchant_identifier(
            self.config.as_ref(),
            crate::in_manual::InIdentifier::MobileOrVpa,
        )
    }
}

impl Default for PhonepeGateway {
    fn default() -> Self {
        Self::new_unconfigured()
    }
}

#[async_trait]
impl PaymentGateway for PhonepeGateway {
    fn id(&self) -> GatewayId {
        GatewayId::Phonepe
    }

    fn display_name(&self) -> &str {
        "PhonePe"
    }

    fn is_configured(&self) -> bool {
        self.merchant_number().is_some()
    }

    async fn initiate(&self, req: &PaymentRequest) -> Result<InitiatedPayment, GatewayError> {
        // Wave 30: shared IN-manual gate (identifier shape + INR-only).
        crate::in_manual::checked_manual_initiate_in(
            "phonepe",
            "PhonePe",
            crate::in_manual::InIdentifier::MobileOrVpa,
            self.config.as_ref(),
            req,
        )
    }

    async fn verify(&self, provider_ref: &str) -> Result<VerifyResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("phonepe".into()));
        }

        // PhonePe manual payments have no provider API to poll here: real
        // verification happens via the SMS verification engine, which
        // matches the confirmation SMS sent to the merchant's phone
        // against the pending payment. Until that engine reports a
        // match, nothing is verified, so this always returns an
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
        Err(GatewayError::Unsupported(
            "phonepe refunds are manual".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miraclepay_domain::{Currency, Money};

    fn config(enabled: bool, merchant_number: Option<&str>) -> GatewayConfig {
        GatewayConfig {
            gateway: GatewayId::Phonepe,
            merchant_number: merchant_number.map(str::to_string),
            api_key: None,
            api_secret: None,
            extra: serde_json::Value::Null,
            sandbox: false,
            enabled,
        }
    }

    fn configured_gateway() -> PhonepeGateway {
        PhonepeGateway::with_config(config(true, Some("9876543210")))
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
        let gateway = PhonepeGateway::new_unconfigured();
        assert_eq!(gateway.id(), GatewayId::Phonepe);
        assert_eq!(gateway.display_name(), "PhonePe");
        assert!(!gateway.is_configured());
    }

    #[test]
    fn disabled_or_numberless_config_is_not_configured() {
        let disabled = PhonepeGateway::with_config(config(false, Some("9876543210")));
        assert!(!disabled.is_configured());

        let numberless = PhonepeGateway::with_config(config(true, None));
        assert!(!numberless.is_configured());

        assert!(configured_gateway().is_configured());
    }

    #[tokio::test]
    async fn unconfigured_initiate_is_not_configured_error() {
        let gateway = PhonepeGateway::new_unconfigured();
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("unconfigured gateway must not initiate a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "phonepe"),
            "expected NotConfigured(\"phonepe\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn unconfigured_verify_is_not_configured_error() {
        let gateway = PhonepeGateway::new_unconfigured();
        let err = gateway
            .verify("MP-TEST-001")
            .await
            .expect_err("unconfigured gateway must not verify a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "phonepe"),
            "expected NotConfigured(\"phonepe\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn configured_initiate_instructions_contain_merchant_number() {
        let gateway = configured_gateway();

        let initiated = gateway
            .initiate(&sample_request())
            .await
            .expect("configured gateway should initiate a payment");

        assert_eq!(initiated.provider_ref, "MP-TEST-001");
        assert_eq!(initiated.redirect_url, None);
        assert!(initiated.expires_at.is_none());

        let instructions = initiated
            .instructions
            .expect("manual gateway must return payment instructions");
        assert!(
            instructions.contains("9876543210"),
            "instructions must contain the merchant number, got: {instructions}"
        );
        assert!(
            instructions.contains("PhonePe"),
            "instructions must name the gateway, got: {instructions}"
        );
        assert!(
            instructions.contains("INR 500.00"),
            "instructions must contain the formatted amount, got: {instructions}"
        );
        assert!(
            instructions.contains("MP-TEST-001"),
            "instructions must contain the payment reference, got: {instructions}"
        );
    }

    #[tokio::test]
    async fn configured_verify_is_unverified_until_sms_engine_matches() {
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
    async fn refund_is_unsupported() {
        let gateway = configured_gateway();
        let err = gateway
            .refund(&RefundRequest {
                provider_ref: "MP-TEST-001".to_string(),
                amount_minor: 50_000,
                reason: None,
            })
            .await
            .expect_err("PhonePe refunds are manual, never automatic");
        assert!(
            matches!(&err, GatewayError::Unsupported(msg) if msg == "phonepe refunds are manual"),
            "expected Unsupported(\"phonepe refunds are manual\"), got: {err:?}"
        );
    }
}
