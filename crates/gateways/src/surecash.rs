//! SureCash gateway for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! SureCash is a *manual* Bangladeshi gateway in this version: there is no
//! hosted checkout redirect. Initiating a payment returns instructions
//! telling the customer to send money to the merchant's SureCash number
//! and to quote the payment reference. The payment is only ever marked
//! verified later, by the SMS verification engine matching an incoming
//! SureCash SMS against the pending payment — never by this module.
//! Refunds are likewise handled manually by the merchant, outside the
//! gateway.

use async_trait::async_trait;

use crate::traits::{manual_instructions, PaymentGateway};
use crate::types::{
    GatewayConfig, GatewayError, GatewayId, InitiatedPayment, PaymentRequest, RefundRequest,
    RefundResult, VerifyResult,
};

/// Payment gateway for SureCash (manual send-money flow).
///
/// Holds an optional [`GatewayConfig`]; a gateway built with
/// [`SurecashGateway::new_unconfigured`] (or with a disabled / incomplete
/// config) reports itself as not configured and refuses to initiate or
/// verify payments.
pub struct SurecashGateway {
    config: Option<GatewayConfig>,
}

impl SurecashGateway {
    /// A gateway with no configuration. Every operation that needs a
    /// configuration fails with [`GatewayError::NotConfigured`].
    pub fn new_unconfigured() -> Self {
        SurecashGateway { config: None }
    }

    /// A gateway using the given configuration.
    pub fn with_config(config: GatewayConfig) -> Self {
        SurecashGateway {
            config: Some(config),
        }
    }

    /// The configured merchant SureCash number, when the gateway is fully
    /// configured: a config is present, it is enabled, and it carries a
    /// non-empty merchant number.
    fn merchant_number(&self) -> Option<&str> {
        self.config
            .as_ref()
            .filter(|config| config.enabled)
            .and_then(|config| config.merchant_number.as_deref())
            .filter(|number| !number.trim().is_empty())
    }
}

impl Default for SurecashGateway {
    fn default() -> Self {
        Self::new_unconfigured()
    }
}

#[async_trait]
impl PaymentGateway for SurecashGateway {
    fn id(&self) -> GatewayId {
        GatewayId::Surecash
    }

    fn display_name(&self) -> &str {
        "SureCash"
    }

    fn is_configured(&self) -> bool {
        self.merchant_number().is_some()
    }

    async fn initiate(&self, req: &PaymentRequest) -> Result<InitiatedPayment, GatewayError> {
        let merchant_number = self
            .merchant_number()
            .ok_or_else(|| GatewayError::NotConfigured("surecash".into()))?;

        Ok(InitiatedPayment {
            provider_ref: req.reference.clone(),
            redirect_url: None,
            instructions: Some(manual_instructions(
                "SureCash",
                merchant_number,
                &req.amount.format_display(),
                &req.reference,
            )),
            expires_at: None,
        })
    }

    async fn verify(&self, provider_ref: &str) -> Result<VerifyResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("surecash".into()));
        }

        // SureCash manual payments have no provider API to poll here: real
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
            "surecash refunds are manual".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miraclepay_domain::{Currency, Money};

    fn config(enabled: bool, merchant_number: Option<&str>) -> GatewayConfig {
        GatewayConfig {
            gateway: GatewayId::Surecash,
            merchant_number: merchant_number.map(str::to_string),
            api_key: None,
            api_secret: None,
            extra: serde_json::Value::Null,
            sandbox: false,
            enabled,
        }
    }

    fn configured_gateway() -> SurecashGateway {
        SurecashGateway::with_config(config(true, Some("01700000000")))
    }

    fn sample_request() -> PaymentRequest {
        PaymentRequest {
            merchant_id: "merchant-1".to_string(),
            amount: Money::new(50_000, Currency::BDT),
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
        let gateway = SurecashGateway::new_unconfigured();
        assert_eq!(gateway.id(), GatewayId::Surecash);
        assert_eq!(gateway.display_name(), "SureCash");
        assert!(!gateway.is_configured());
    }

    #[test]
    fn disabled_or_numberless_config_is_not_configured() {
        let disabled = SurecashGateway::with_config(config(false, Some("01700000000")));
        assert!(!disabled.is_configured());

        let numberless = SurecashGateway::with_config(config(true, None));
        assert!(!numberless.is_configured());

        assert!(configured_gateway().is_configured());
    }

    #[tokio::test]
    async fn unconfigured_initiate_is_not_configured_error() {
        let gateway = SurecashGateway::new_unconfigured();
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("unconfigured gateway must not initiate a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "surecash"),
            "expected NotConfigured(\"surecash\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn unconfigured_verify_is_not_configured_error() {
        let gateway = SurecashGateway::new_unconfigured();
        let err = gateway
            .verify("MP-TEST-001")
            .await
            .expect_err("unconfigured gateway must not verify a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "surecash"),
            "expected NotConfigured(\"surecash\"), got: {err:?}"
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
            instructions.contains("01700000000"),
            "instructions must contain the merchant number, got: {instructions}"
        );
        assert!(
            instructions.contains("SureCash"),
            "instructions must name the gateway, got: {instructions}"
        );
        assert!(
            instructions.contains("BDT 500.00"),
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
            .expect_err("SureCash refunds are manual, never automatic");
        assert!(
            matches!(&err, GatewayError::Unsupported(msg) if msg == "surecash refunds are manual"),
            "expected Unsupported(\"surecash refunds are manual\"), got: {err:?}"
        );
    }
}
