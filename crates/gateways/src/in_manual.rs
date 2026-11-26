//! Shared hardening for the Indian manual gateways (Wave 30).
//!
//! UPI, Paytm, and PhonePe are manual send-money gateways in
//! v0.0.1: the customer sends INR to the merchant's UPI VPA or
//! Indian mobile number and the SMS engine verifies. As the PK
//! adapters did before Wave 29, these accepted any non-empty
//! identifier and any currency. This module is the single
//! validation point:
//!
//! - [`valid_in_mobile`]: Indian mobile — 10 digits starting 6–9,
//!   optionally `+91`/`91` prefixed.
//! - [`valid_vpa`]: UPI virtual payment address —
//!   `local@handle` where local is ≥2 chars of `[A-Za-z0-9._-]` and
//!   the handle is ≥2 alphanumeric chars (okhdfcbank, ybl, paytm…).
//!   The previous check was merely "contains @".
//! - [`checked_manual_initiate_in`]: config + INR-only gate.

use miraclepay_domain::Currency;

use crate::traits::manual_instructions;
use crate::types::{GatewayConfig, GatewayError, InitiatedPayment, PaymentRequest};

/// Which identifier a gateway takes as its merchant handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InIdentifier {
    /// UPI: a VPA is required.
    Vpa,
    /// Paytm / PhonePe: an Indian mobile number or a VPA.
    MobileOrVpa,
}

/// Whether `number` is shaped like an Indian mobile number.
pub fn valid_in_mobile(number: &str) -> bool {
    let digits: String = number.chars().filter(|c| *c != ' ' && *c != '-').collect();
    let digits = digits.strip_prefix('+').unwrap_or(&digits);
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // Local: 10 digits starting 6-9. International: 91 + local.
    let local = if digits.len() == 12 && digits.starts_with("91") {
        &digits[2..]
    } else {
        digits
    };
    local.len() == 10 && matches!(local.as_bytes()[0], b'6'..=b'9')
}

/// Whether `vpa` is a well-formed UPI virtual payment address.
pub fn valid_vpa(vpa: &str) -> bool {
    let Some((local, handle)) = vpa.split_once('@') else {
        return false;
    };
    if vpa.matches('@').count() != 1 {
        return false;
    }
    local.len() >= 2
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && handle.len() >= 2
        && handle.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Whether `identifier` satisfies `kind`.
pub fn valid_identifier(kind: InIdentifier, identifier: &str) -> bool {
    match kind {
        InIdentifier::Vpa => valid_vpa(identifier),
        InIdentifier::MobileOrVpa => valid_in_mobile(identifier) || valid_vpa(identifier),
    }
}

/// The merchant identifier, when configured + valid for `kind`.
pub fn merchant_identifier(config: Option<&GatewayConfig>, kind: InIdentifier) -> Option<&str> {
    config
        .filter(|config| config.enabled)
        .and_then(|config| config.merchant_number.as_deref())
        .filter(|identifier| valid_identifier(kind, identifier))
}

/// `Some(reason)` when an identifier is present but malformed
/// (→ `InvalidConfig`); `None` when simply unconfigured.
pub fn config_problem(
    config: Option<&GatewayConfig>,
    kind: InIdentifier,
    gateway: &str,
) -> Option<String> {
    let config = config?;
    if !config.enabled {
        return None;
    }
    match config.merchant_number.as_deref() {
        Some(id) if !id.trim().is_empty() && !valid_identifier(kind, id) => Some(format!(
            "{gateway} merchant identifier must be {}",
            match kind {
                InIdentifier::Vpa => "a UPI VPA (e.g. shop@okhdfcbank)",
                InIdentifier::MobileOrVpa => {
                    "an Indian mobile (98XXXXXXXX) or a UPI VPA"
                }
            }
        )),
        _ => None,
    }
}

/// Shared `initiate` for UPI/Paytm/PhonePe: config validity +
/// **INR-only** amounts before producing instructions.
pub fn checked_manual_initiate_in(
    gateway: &str,
    display_name: &str,
    kind: InIdentifier,
    config: Option<&GatewayConfig>,
    req: &PaymentRequest,
) -> Result<InitiatedPayment, GatewayError> {
    if let Some(problem) = config_problem(config, kind, gateway) {
        return Err(GatewayError::InvalidConfig(problem));
    }
    let identifier = merchant_identifier(config, kind)
        .ok_or_else(|| GatewayError::NotConfigured(gateway.into()))?;
    if req.amount.currency != Currency::INR {
        return Err(GatewayError::Provider(format!(
            "{gateway} payments must be in INR, got {}",
            req.amount.currency.code()
        )));
    }
    Ok(InitiatedPayment {
        provider_ref: req.reference.clone(),
        redirect_url: None,
        instructions: Some(manual_instructions(
            display_name,
            identifier,
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
    fn in_mobile_shapes() {
        assert!(valid_in_mobile("9876543210"));
        assert!(valid_in_mobile("+91 98765 43210"));
        assert!(valid_in_mobile("919876543210"));
        assert!(!valid_in_mobile("5876543210")); // starts with 5
        assert!(!valid_in_mobile("01712345678")); // Bangladeshi
        assert!(!valid_in_mobile("987654321")); // too short
        assert!(!valid_in_mobile(""));
    }

    #[test]
    fn vpa_shapes() {
        assert!(valid_vpa("shop@okhdfcbank"));
        assert!(valid_vpa("9876543210@paytm"));
        assert!(valid_vpa("my.shop-1@ybl"));
        assert!(!valid_vpa("shop")); // no handle
        assert!(!valid_vpa("a@ybl")); // local too short
        assert!(!valid_vpa("shop@")); // empty handle
        assert!(!valid_vpa("sh op@ybl")); // space
        assert!(!valid_vpa("a@b@c")); // two @
    }
}
