//! Settings domain types for MiraclePay.
//!
//! Original clean-room implementation. Settings exist at two scopes:
//!
//! - [`PlatformSettings`] — platform-wide values owned by the operator:
//!   the defaults every merchant inherits unless they override them, plus
//!   global switches such as maintenance mode.
//! - [`MerchantSettings`] — per-merchant overrides and preferences. A
//!   field left unset falls back to the platform-wide value (for example
//!   `fee_bps: None` means "charge this merchant the platform fee").
//!
//! Fees are expressed in basis points (bps): 1 bp = 0.01%, so
//! 10_000 bps = 100%. Amounts (such as a settlement threshold) are integer
//! counts of a currency's minor unit, never floating point — see
//! [`crate::money`].

use serde::{Deserialize, Serialize};

use crate::money::Currency;

/// Maximum valid fee rate: 10_000 basis points = 100%.
pub const MAX_FEE_BPS: u32 = 10_000;

/// Platform-wide settings, owned by the platform operator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformSettings {
    /// Human-readable name of the platform, shown in checkout and mail.
    pub platform_name: String,
    /// Currency used when neither a merchant nor a request picks one.
    pub default_currency: Currency,
    /// Default platform fee in basis points, charged to merchants that
    /// do not have their own `fee_bps` override.
    pub default_fee_bps: u32,
    /// Maximum number of times a webhook delivery is attempted before
    /// the delivery is given up on.
    pub webhook_max_attempts: u32,
    /// How long a checkout session stays valid, in minutes.
    pub checkout_session_minutes: i64,
    /// When `true`, the platform refuses new payments for maintenance.
    pub maintenance_mode: bool,
    /// Public support contact email shown to merchants and customers.
    pub support_email: String,
    /// FX rates against BDT for reporting conversions (Wave 34;
    /// see `crate::fx`). Defaults to the built-in indicative table.
    #[serde(default = "crate::fx::default_rates")]
    pub fx_rates: Vec<crate::fx::FxRate>,
}

impl Default for PlatformSettings {
    /// The shipped defaults for MiraclePay v0.0.1: BDT as the default
    /// currency, a 250 bps (2.5%) platform fee, 8 webhook delivery
    /// attempts, 30-minute checkout sessions, maintenance mode off.
    fn default() -> Self {
        Self {
            platform_name: "MiraclePay".to_string(),
            default_currency: Currency::BDT,
            default_fee_bps: 250,
            webhook_max_attempts: 8,
            checkout_session_minutes: 30,
            maintenance_mode: false,
            support_email: "support@example.com".to_string(),
            fx_rates: crate::fx::default_rates(),
        }
    }
}

/// Per-merchant settings: overrides of, and preferences alongside, the
/// platform-wide [`PlatformSettings`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerchantSettings {
    /// Identifier of the merchant these settings belong to.
    pub merchant_id: String,
    /// This merchant's fee in basis points, overriding the platform
    /// default. `None` means the platform `default_fee_bps` applies.
    pub fee_bps: Option<u32>,
    /// Whether settlements run automatically once the merchant's
    /// balance reaches `settle_threshold_minor`.
    pub auto_settle: bool,
    /// Minimum balance, in minor units, that triggers an automatic
    /// settlement when `auto_settle` is enabled. Must not be negative.
    pub settle_threshold_minor: i64,
    /// Send the merchant a notification when a payment succeeds.
    pub notify_on_payment: bool,
    /// Send the merchant a notification when a payment is refunded.
    pub notify_on_refund: bool,
}

impl MerchantSettings {
    /// The fee rate that actually applies to this merchant: its own
    /// `fee_bps` override when set, otherwise the platform default.
    pub fn effective_fee_bps(&self, platform: &PlatformSettings) -> u32 {
        self.fee_bps.unwrap_or(platform.default_fee_bps)
    }

    /// Validate these settings; see [`validate_merchant_settings`].
    pub fn validate(&self) -> Result<(), Vec<String>> {
        validate_merchant_settings(self)
    }
}

/// Validate a [`MerchantSettings`].
///
/// Returns `Ok(())` when the settings are acceptable, or `Err`
/// containing *all* validation problems found (one human-readable
/// message per problem), so callers can surface every issue at once.
///
/// Rules:
/// - `fee_bps`, when present, must be at most 10_000 (100%). `None`
///   (inherit the platform fee) is always valid.
/// - `settle_threshold_minor` must be greater than or equal to 0.
pub fn validate_merchant_settings(settings: &MerchantSettings) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    if let Some(fee_bps) = settings.fee_bps {
        if fee_bps > MAX_FEE_BPS {
            errors.push(format!(
                "fee_bps must be at most {MAX_FEE_BPS} (100%), got {fee_bps}"
            ));
        }
    }

    if settings.settle_threshold_minor < 0 {
        errors.push(format!(
            "settle_threshold_minor must be greater than or equal to 0, got {}",
            settings.settle_threshold_minor
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_settings() -> MerchantSettings {
        MerchantSettings {
            merchant_id: "mch_1".to_string(),
            fee_bps: None,
            auto_settle: true,
            settle_threshold_minor: 100_00,
            notify_on_payment: true,
            notify_on_refund: true,
        }
    }

    #[test]
    fn platform_defaults_match_specification() {
        let settings = PlatformSettings::default();
        assert_eq!(settings.platform_name, "MiraclePay");
        assert_eq!(settings.default_currency, Currency::BDT);
        assert_eq!(settings.default_fee_bps, 250);
        assert_eq!(settings.webhook_max_attempts, 8);
        assert_eq!(settings.checkout_session_minutes, 30);
        assert!(!settings.maintenance_mode);
        assert_eq!(settings.support_email, "support@example.com");
    }

    #[test]
    fn platform_settings_construct_with_all_fields() {
        let settings = PlatformSettings {
            platform_name: "MiraclePay".to_string(),
            default_currency: Currency::USD,
            default_fee_bps: 300,
            webhook_max_attempts: 5,
            checkout_session_minutes: 15,
            maintenance_mode: true,
            support_email: "help@example.com".to_string(),
            fx_rates: crate::fx::default_rates(),
        };
        assert_eq!(settings.default_currency, Currency::USD);
        assert!(settings.maintenance_mode);
        assert_eq!(settings.clone(), settings);
    }

    #[test]
    fn valid_merchant_settings_pass() {
        assert_eq!(validate_merchant_settings(&valid_settings()), Ok(()));
        assert_eq!(valid_settings().validate(), Ok(()));
    }

    #[test]
    fn fee_bps_none_and_boundaries_pass() {
        // None inherits the platform fee and is always valid.
        let mut settings = valid_settings();
        settings.fee_bps = None;
        assert_eq!(validate_merchant_settings(&settings), Ok(()));

        // 0% and exactly 100% are both legitimate rates.
        for fee in [0, 1, 250, 9_999, MAX_FEE_BPS] {
            let mut settings = valid_settings();
            settings.fee_bps = Some(fee);
            assert_eq!(
                validate_merchant_settings(&settings),
                Ok(()),
                "fee_bps {fee} must pass"
            );
        }
    }

    #[test]
    fn fee_bps_above_one_hundred_percent_is_rejected() {
        for fee in [MAX_FEE_BPS + 1, 20_000, u32::MAX] {
            let mut settings = valid_settings();
            settings.fee_bps = Some(fee);
            let errors = validate_merchant_settings(&settings).expect_err("must fail");
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert!(errors[0].contains("fee_bps"), "{errors:?}");
        }
    }

    #[test]
    fn zero_settle_threshold_passes() {
        let mut settings = valid_settings();
        settings.settle_threshold_minor = 0;
        assert_eq!(validate_merchant_settings(&settings), Ok(()));
    }

    #[test]
    fn negative_settle_threshold_is_rejected() {
        for bad in [-1, -100_00, i64::MIN] {
            let mut settings = valid_settings();
            settings.settle_threshold_minor = bad;
            let errors = validate_merchant_settings(&settings).expect_err("must fail");
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert!(
                errors[0].contains("settle_threshold_minor"),
                "{errors:?}"
            );
        }
    }

    #[test]
    fn multiple_problems_are_all_reported() {
        let mut settings = valid_settings();
        settings.fee_bps = Some(MAX_FEE_BPS + 1);
        settings.settle_threshold_minor = -1;
        let errors = validate_merchant_settings(&settings).expect_err("must fail");
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("fee_bps")), "{errors:?}");
        assert!(
            errors.iter().any(|e| e.contains("settle_threshold_minor")),
            "{errors:?}"
        );
    }

    #[test]
    fn effective_fee_bps_prefers_merchant_override() {
        let platform = PlatformSettings::default();

        let mut settings = valid_settings();
        settings.fee_bps = Some(150);
        assert_eq!(settings.effective_fee_bps(&platform), 150);

        settings.fee_bps = None;
        assert_eq!(settings.effective_fee_bps(&platform), 250);
        assert_eq!(
            settings.effective_fee_bps(&platform),
            platform.default_fee_bps
        );
    }

    #[test]
    fn merchant_settings_construct_with_all_fields() {
        let settings = valid_settings();
        assert_eq!(settings.merchant_id, "mch_1");
        assert!(settings.auto_settle);
        assert!(settings.notify_on_payment);
        assert!(settings.notify_on_refund);
        assert_eq!(settings.clone(), settings);
    }

    #[test]
    fn serde_round_trip() {
        let platform = PlatformSettings::default();
        let json = serde_json::to_string(&platform).expect("serialize platform");
        assert_eq!(
            serde_json::from_str::<PlatformSettings>(&json).expect("deserialize platform"),
            platform
        );

        let merchant = valid_settings();
        let json = serde_json::to_string(&merchant).expect("serialize merchant");
        assert_eq!(
            serde_json::from_str::<MerchantSettings>(&json).expect("deserialize merchant"),
            merchant
        );
    }
}
