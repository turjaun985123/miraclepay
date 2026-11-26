//! Shared hardening for the Pakistani manual gateways (Wave 29).
//!
//! Easypaisa and JazzCash are manual send-money gateways: the
//! customer sends PKR to the merchant's Pakistani mobile number and
//! the SMS engine verifies. Both adapters previously accepted *any*
//! non-empty merchant number and *any* currency — a BDT payment to a
//! Bangladeshi-format number would have produced confident-looking
//! but wrong instructions. This module is the single place that
//! validates both, per the project's reusable-unit rule:
//!
//! - [`valid_pk_mobile`]: Pakistani mobile shape — local
//!   `03XXXXXXXXX` (11 digits) or international `+923XXXXXXXXX` /
//!   `923XXXXXXXXX` (12 digits), spaces and dashes tolerated.
//! - [`checked_manual_initiate`]: config + currency gate used by
//!   both adapters' `initiate`.

use miraclepay_domain::Currency;

use crate::traits::manual_instructions;
use crate::types::{GatewayConfig, GatewayError, InitiatedPayment, PaymentRequest};

/// Whether `number` is shaped like a Pakistani mobile number.
pub fn valid_pk_mobile(number: &str) -> bool {
    let digits: String = number
        .chars()
        .filter(|c| *c != ' ' && *c != '-')
        .collect();
    let digits = digits.strip_prefix('+').unwrap_or(&digits);
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // Local form: 03XXXXXXXXX (11 digits). International: 923XXXXXXXXX.
    (digits.len() == 11 && digits.starts_with("03"))
        || (digits.len() == 12 && digits.starts_with("923"))
}

/// The merchant number to pay, when the config is present, enabled,
/// and carries a *valid Pakistani mobile* number. A present but
/// malformed number is a configuration error, not "unconfigured" —
/// callers distinguish the two via [`config_problem`].
pub fn merchant_number<'a>(config: Option<&'a GatewayConfig>) -> Option<&'a str> {
    config
        .filter(|config| config.enabled)
        .and_then(|config| config.merchant_number.as_deref())
        .filter(|number| valid_pk_mobile(number))
}

/// Why a config cannot be used: `Some(reason)` when a number is
/// present but malformed (→ `InvalidConfig`), `None` when the
/// gateway is simply not configured (→ `NotConfigured`).
pub fn config_problem(config: Option<&GatewayConfig>, gateway: &str) -> Option<String> {
    let config = config?;
    if !config.enabled {
        return None;
    }
    match config.merchant_number.as_deref() {
        Some(number) if !number.trim().is_empty() && !valid_pk_mobile(number) => Some(format!(
            "{gateway} merchant number must be a Pakistani mobile (03XXXXXXXXX or +923XXXXXXXXX)"
        )),
        _ => None,
    }
}

/// Shared `initiate` for Easypaisa/JazzCash: enforces configuration
/// validity and **PKR-only** amounts before producing instructions.
pub fn checked_manual_initiate(
    gateway: &str,
    display_name: &str,
    config: Option<&GatewayConfig>,
    req: &PaymentRequest,
) -> Result<InitiatedPayment, GatewayError> {
    if let Some(problem) = config_problem(config, gateway) {
        return Err(GatewayError::InvalidConfig(problem));
    }
    let merchant_number = merchant_number(config)
        .ok_or_else(|| GatewayError::NotConfigured(gateway.into()))?;
    if req.amount.currency != Currency::PKR {
        return Err(GatewayError::Provider(format!(
            "{gateway} payments must be in PKR, got {}",
            req.amount.currency.code()
        )));
    }
    Ok(InitiatedPayment {
        provider_ref: req.reference.clone(),
        redirect_url: None,
        instructions: Some(manual_instructions(
            display_name,
            merchant_number,
            &req.amount.format_display(),
            &req.reference,
        )),
        expires_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pk_mobile_shapes() {
        assert!(valid_pk_mobile("03001234567"));
        assert!(valid_pk_mobile("+92 300 1234567"));
        assert!(valid_pk_mobile("923001234567"));
        assert!(valid_pk_mobile("0300-1234567"));
        // Bangladeshi number, landline, too short, wrong prefix.
        assert!(!valid_pk_mobile("01712345678"));
        assert!(!valid_pk_mobile("04235781234"));
        assert!(!valid_pk_mobile("0300123456"));
        assert!(!valid_pk_mobile("030012345678"));
        assert!(!valid_pk_mobile(""));
    }
}
