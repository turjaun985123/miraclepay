//! Manual gateway for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! The manual gateway is the catch-all channel for payments that
//! happen entirely out of band: a bank transfer, cash handed over in
//! person, or any custom payment method a merchant arranges with
//! their customers directly. There is no provider, no hosted
//! checkout redirect, and no API to call.
//!
//! Initiating a payment simply returns instructions telling the
//! customer how to pay. A merchant may supply their own instructions
//! verbatim (in the gateway config's `extra["instructions"]`); when
//! they do not, generic instructions are built with
//! [`manual_instructions`]. The payment is only ever settled later,
//! by the merchant reviewing the incoming transfer themselves —
//! never by this module. Refunds are likewise handled manually by
//! the merchant, outside the gateway.

use async_trait::async_trait;

use crate::traits::{manual_instructions, PaymentGateway};
use crate::types::{
    GatewayConfig, GatewayError, GatewayId, InitiatedPayment, PaymentRequest, RefundRequest,
    RefundResult, VerifyResult,
};

/// Payment gateway for manual payments (bank transfer / cash /
/// custom methods).
///
/// Holds an optional [`GatewayConfig`]; a gateway built with
/// [`ManualGateway::new_unconfigured`] (or with a disabled config)
/// reports itself as not configured and refuses to initiate or
/// verify payments.
///
/// Unlike the mobile-money gateways, a merchant number is
/// *optional* here: a bank account, a cash desk, or a custom method
/// may have no phone-style number at all, so a present, enabled
/// config is enough to be configured.
pub struct ManualGateway {
    config: Option<GatewayConfig>,
}

impl ManualGateway {
    /// A gateway with no configuration. Every operation that needs a
    /// configuration fails with [`GatewayError::NotConfigured`].
    pub fn new_unconfigured() -> Self {
        ManualGateway { config: None }
    }

    /// A gateway using the given configuration.
    pub fn with_config(config: GatewayConfig) -> Self {
        ManualGateway {
            config: Some(config),
        }
    }

    /// The enabled configuration, when one is present.
    fn enabled_config(&self) -> Option<&GatewayConfig> {
        self.config.as_ref().filter(|config| config.enabled)
    }

    /// The payer-facing instructions for a payment: the merchant's
    /// own instructions from `extra["instructions"]`, used verbatim
    /// when present and non-empty, otherwise generic instructions
    /// built from the merchant number (or a generic placeholder when
    /// no number is configured), the formatted amount, and the
    /// payment reference.
    fn instructions(&self, config: &GatewayConfig, req: &PaymentRequest) -> String {
        if let Some(custom) = config
            .extra
            .get("instructions")
            .and_then(serde_json::Value::as_str)
            .filter(|text| !text.trim().is_empty())
        {
            return custom.to_string();
        }

        let merchant_number = config
            .merchant_number
            .as_deref()
            .filter(|number| !number.trim().is_empty())
            .unwrap_or("the merchant");

        manual_instructions(
            "Manual payment",
            merchant_number,
            &req.amount.format_display(),
            &req.reference,
        )
    }
}

impl Default for ManualGateway {
    fn default() -> Self {
        Self::new_unconfigured()
    }
}

#[async_trait]
impl PaymentGateway for ManualGateway {
    fn id(&self) -> GatewayId {
        GatewayId::Manual
    }

    fn display_name(&self) -> &str {
        "Manual"
    }

    fn is_configured(&self) -> bool {
        self.enabled_config().is_some()
    }

    async fn initiate(&self, req: &PaymentRequest) -> Result<InitiatedPayment, GatewayError> {
        let config = self
            .enabled_config()
            .ok_or_else(|| GatewayError::NotConfigured("manual".into()))?;

        let instructions = self.instructions(config, req);

        Ok(InitiatedPayment {
            provider_ref: req.reference.clone(),
            redirect_url: None,
            instructions: Some(instructions),
            expires_at: None,
        })
    }

    async fn verify(&self, provider_ref: &str) -> Result<VerifyResult, GatewayError> {
        if !self.is_configured() {
            return Err(GatewayError::NotConfigured("manual".into()));
        }

        // Manual payments have no provider to ask: verification is a
        // manual review by the merchant, who checks their bank
        // account / cash receipts against the payment reference and
        // settles the payment themselves. Until the merchant does
        // that, nothing is verified, so this always returns an
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
            "manual refunds are handled by the merchant".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miraclepay_domain::{Currency, Money};

    fn config(enabled: bool, merchant_number: Option<&str>) -> GatewayConfig {
        GatewayConfig {
            gateway: GatewayId::Manual,
            merchant_number: merchant_number.map(str::to_string),
            api_key: None,
            api_secret: None,
            extra: serde_json::Value::Null,
            sandbox: false,
            enabled,
        }
    }

    fn configured_gateway() -> ManualGateway {
        ManualGateway::with_config(config(true, Some("1234567890")))
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
        let gateway = ManualGateway::new_unconfigured();
        assert_eq!(gateway.id(), GatewayId::Manual);
        assert_eq!(gateway.display_name(), "Manual");
        assert!(!gateway.is_configured());

        let default_gateway = ManualGateway::default();
        assert!(!default_gateway.is_configured());
    }

    #[test]
    fn disabled_config_is_not_configured() {
        let disabled = ManualGateway::with_config(config(false, Some("1234567890")));
        assert!(!disabled.is_configured());

        // A merchant number is optional for manual payments: a
        // present, enabled config is configured without one.
        let numberless = ManualGateway::with_config(config(true, None));
        assert!(numberless.is_configured());

        assert!(configured_gateway().is_configured());
    }

    #[tokio::test]
    async fn unconfigured_operations_are_not_configured_errors() {
        let gateway = ManualGateway::new_unconfigured();

        let err = gateway
            .initiate(&sample_request())
            .await
            .expect_err("unconfigured gateway must not initiate a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "manual"),
            "expected NotConfigured(\"manual\"), got: {err:?}"
        );

        let err = gateway
            .verify("MP-TEST-001")
            .await
            .expect_err("unconfigured gateway must not verify a payment");
        assert!(
            matches!(&err, GatewayError::NotConfigured(name) if name == "manual"),
            "expected NotConfigured(\"manual\"), got: {err:?}"
        );
    }

    #[tokio::test]
    async fn custom_instructions_from_extra_are_used_verbatim() {
        let mut custom_config = config(true, Some("1234567890"));
        custom_config.extra = serde_json::json!({
            "instructions": "Pay by bank transfer to account 1234567890, then call us."
        });
        let gateway = ManualGateway::with_config(custom_config);

        let initiated = gateway
            .initiate(&sample_request())
            .await
            .expect("configured gateway should initiate a payment");

        assert_eq!(initiated.provider_ref, "MP-TEST-001");
        assert_eq!(initiated.redirect_url, None);
        assert!(initiated.expires_at.is_none());
        assert_eq!(
            initiated.instructions.as_deref(),
            Some("Pay by bank transfer to account 1234567890, then call us."),
            "custom instructions must be returned verbatim"
        );
    }

    #[tokio::test]
    async fn fallback_instructions_contain_reference() {
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
            instructions.contains("MP-TEST-001"),
            "instructions must contain the payment reference, got: {instructions}"
        );
        assert!(
            instructions.contains("1234567890"),
            "instructions must contain the merchant number, got: {instructions}"
        );
        assert!(
            instructions.contains("Manual payment"),
            "instructions must name the payment method, got: {instructions}"
        );
        assert!(
            instructions.contains("BDT 500.00"),
            "instructions must contain the formatted amount, got: {instructions}"
        );

        // With no merchant number configured, the fallback
        // instructions address a generic merchant instead.
        let numberless = ManualGateway::with_config(config(true, None));
        let initiated = numberless
            .initiate(&sample_request())
            .await
            .expect("enabled gateway without a number should initiate a payment");
        let instructions = initiated
            .instructions
            .expect("manual gateway must return payment instructions");
        assert!(
            instructions.contains("the merchant"),
            "instructions must fall back to a generic merchant, got: {instructions}"
        );
        assert!(
            instructions.contains("MP-TEST-001"),
            "instructions must contain the payment reference, got: {instructions}"
        );
    }

    #[tokio::test]
    async fn configured_verify_is_unverified_pending_manual_review() {
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
            .expect_err("manual refunds are handled by the merchant, never automatic");
        assert!(
            matches!(&err, GatewayError::Unsupported(msg) if msg == "manual refunds are handled by the merchant"),
            "expected Unsupported(\"manual refunds are handled by the merchant\"), got: {err:?}"
        );
    }
}
