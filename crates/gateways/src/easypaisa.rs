//! Easypaisa gateway for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! Easypaisa is a *manual* Pakistani gateway in this version: there is no
//! hosted checkout redirect. Initiating a payment returns instructions
//! telling the customer to send money to the merchant's Easypaisa number
//! and to quote the payment reference. The payment is only ever marked
//! verified later, by the SMS verification engine matching an incoming
//! Easypaisa SMS against the pending payment — never by this module.
//! Refunds are likewise handled manually by the merchant, outside the
//! gateway.

use async_trait::async_trait;

use crate::traits::PaymentGateway;
use crate::types::{
    GatewayConfig, GatewayError, GatewayId, InitiatedPayment, PaymentRequest, RefundRequest,
    RefundResult, VerifyResult,
};

/// Payment gateway for Easypaisa (manual send-money flow).
///
/// Holds an optional [`GatewayConfig`]; a gateway built with
/// [`EasypaisaGateway::new_unconfigured`] (or with a disabled / incomplete
/// config) reports itself as not configured and refuses to initiate or
/// verify payments.
pub struct EasypaisaGateway {
    config: Option<GatewayConfig>,
}

impl EasypaisaGateway {
    /// A gateway with no configuration. Every operation that needs a
    /// configuration fails with [`GatewayError::NotConfigured`].
    pub fn new_unconfigured() -> Self {
        EasypaisaGateway { config: None }
    }

    /// A gateway using the given configuration.
    pub fn with_config(config: GatewayConfig) -> Self {
        EasypaisaGateway {
            config: Some(config),
        }
    }

    /// The configured merchant Easypaisa number, when the gateway is
    /// fully configured (Wave 29: the number must be a valid
    /// Pakistani mobile — see `pk_manual`).
    fn merchant_number(&self) -> Option<&str> {
        crate::pk_manual::merchant_number(self.config.as_ref())
    }
}

impl Default for EasypaisaGateway {
    fn default() -> Self {
        Self::new_unconfigured()
    }
}

#[async_trait]
impl PaymentGateway for EasypaisaGateway {
    fn id(&self) -> GatewayId {
        GatewayId::Easypaisa
    }

    fn display_name(&self) -> &str {
        "Easypaisa"
    }

    fn is_configured(&self) -> bool {
        self.merchant_number().is_some()
    }

    async fn initiate(&self, req: &PaymentRequest) -> Result<InitiatedPayment, GatewayError> {
        // Wave 29: shared PK-manual gate — valid PK mobile config +
        // PKR-only amounts (see `pk_manual::checked_manual_initiate`).
        crate::pk_manual::checked_manual_initiate(
            "easypaisa",
            "Easypaisa",
            self.config.as_ref(),
            req,
        )
    }

    async fn verify(&self, provider_ref: &str) -> Result<VerifyResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("easypaisa".into()));
        }

        // Easypaisa manual payments have no provider API to poll here: real
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
            "easypaisa refunds are manual".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miraclepay_domain::{Currency, Money};

    fn config(enabled: bool, merchant_number: Option<&str>) -> GatewayConfig {
        GatewayConfig {
            gateway: GatewayId::Easypaisa,
            merchant_number: merchant_number.map(str::to_string),
            api_key: None,
            api_secret: None,
            extra: serde_json::Value::Null,
            sandbox: false,
            enabled,
        }
    }

    fn configured_gateway() -> EasypaisaGateway {
        EasypaisaGateway::with_config(config(true, Some("03001234567")))
    }

    fn sample_request() -> PaymentRequest {
        PaymentRequest {
            merchant_id: "merchant-1".to_string(),
            amount: Money::new(50_000, Currency::PKR),
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
        let gateway = EasypaisaGateway::new_unconfigured();
        assert_eq!(gateway.id(), GatewayId::Easypaisa);
        assert_eq!(gateway.display_name(), "Easypaisa");
        assert!(!gateway.is_configured());
    }

    #[test]
    fn disabled_or_numberless_config_is_not_configured() {
        let disabled = EasypaisaGateway::with_config(config(false, Some("03001234567")));
        assert!(!disabled.is_configured());

        let numberless = EasypaisaGateway::with_config(config(true, None));
        assert!(!numberless.is_configured());

        assert!(configured_gateway().is_configured());
    }

    #[tokio::test]
    async fn unconfigured_initiate_is_not_configured_error() {
        let gateway = EasypaisaGateway::new_unconfigured();
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("unconfigured gateway must not initiate a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "easypaisa"),
            "expected NotConfigured(\"easypaisa\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn unconfigured_verify_is_not_configured_error() {
        let gateway = EasypaisaGateway::new_unconfigured();
        let err = gateway
            .verify("MP-TEST-001")
            .await
            .expect_err("unconfigured gateway must not verify a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "easypaisa"),
            "expected NotConfigured(\"easypaisa\"), got: {err:?}"
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
            instructions.contains("03001234567"),
            "instructions must contain the merchant number, got: {instructions}"
        );
        assert!(
            instructions.contains("Easypaisa"),
            "instructions must name the gateway, got: {instructions}"
        );
        assert!(
            instructions.contains("PKR 500.00"),
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
            .expect_err("Easypaisa refunds are manual, never automatic");
        assert!(
            matches!(&err, GatewayError::Unsupported(msg) if msg == "easypaisa refunds are manual"),
            "expected Unsupported(\"easypaisa refunds are manual\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn malformed_pk_number_is_invalid_config_not_silent() {
        let gateway = EasypaisaGateway::with_config(config(true, Some("01712345678")));
        assert!(!gateway.is_configured());
        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("a Bangladeshi number is not a valid PK merchant number");
        assert!(
            matches!(&err, GatewayError::InvalidConfig(_)),
            "expected InvalidConfig, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn non_pkr_amount_is_rejected() {
        let gateway = configured_gateway();
        let mut req = sample_request();
        req.amount = Money::new(50_000, Currency::BDT);
        let err = gateway
            .initiate(&req)
            .await
            .expect_err("PK gateways must not take BDT payments");
        assert!(
            matches!(&err, GatewayError::Provider(msg) if msg.contains("PKR")),
            "expected Provider(..PKR..), got: {err:?}"
        );
    }
}
