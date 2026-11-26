//! Stripe Checkout Session — request shaping (Wave 9).
//!
//! Original clean-room implementation of the *shape* of Stripe's
//! documented "create a Checkout Session" call:
//! `POST https://api.stripe.com/v1/checkout/sessions`, Bearer auth
//! with the account's secret key, `application/x-www-form-urlencoded`
//! body. Like [`crate::bkash_tokenized`], this module performs no
//! HTTP: it builds an [`HttpRequestSpec`] and parses the response,
//! so the flow is unit-testable without network or credentials.
//!
//! The key comes from the environment (`STRIPE_SECRET_KEY`); a
//! missing key is [`GatewayError::NotConfigured`] and a key that is
//! not a Stripe secret key (`sk_test_…` / `sk_live_…`) is
//! [`GatewayError::InvalidConfig`]. Nothing here fabricates a
//! checkout URL — the URL only ever comes from Stripe's response.

use serde::Deserialize;

use crate::bkash_tokenized::HttpRequestSpec;
use crate::types::GatewayError;

/// Stripe API endpoint for Checkout Sessions.
pub const SESSIONS_URL: &str = "https://api.stripe.com/v1/checkout/sessions";

/// Configuration for the Stripe checkout flow.
#[derive(Debug, Clone)]
pub struct StripeCheckoutConfig {
    /// Stripe secret key (`sk_test_…` or `sk_live_…`).
    pub secret_key: String,
    /// Whether the key is a live-mode key.
    pub live: bool,
}

impl StripeCheckoutConfig {
    /// Build from an explicit variable lookup (env in production,
    /// fixtures in tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, GatewayError> {
        let key = match get("STRIPE_SECRET_KEY") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => {
                return Err(GatewayError::NotConfigured(
                    "stripe: missing env STRIPE_SECRET_KEY".to_string(),
                ))
            }
        };
        if let Some(live) = key.strip_prefix("sk_live_") {
            if live.is_empty() {
                return Err(GatewayError::InvalidConfig("stripe: empty live key".into()));
            }
            Ok(Self {
                secret_key: key,
                live: true,
            })
        } else if let Some(test) = key.strip_prefix("sk_test_") {
            if test.is_empty() {
                return Err(GatewayError::InvalidConfig("stripe: empty test key".into()));
            }
            Ok(Self {
                secret_key: key,
                live: false,
            })
        } else {
            Err(GatewayError::InvalidConfig(
                "stripe: STRIPE_SECRET_KEY must start with sk_test_ or sk_live_".into(),
            ))
        }
    }

    /// Build from the process environment.
    pub fn from_env() -> Result<Self, GatewayError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Shape the create-session request. Amounts are minor units,
    /// exactly as Stripe expects (`unit_amount`).
    pub fn create_session_request(
        &self,
        amount_minor: i64,
        currency: &str,
        product_name: &str,
        success_url: &str,
        cancel_url: &str,
        reference: &str,
    ) -> HttpRequestSpec {
        let fields: [(&str, String); 11] = [
            ("mode", "payment".to_string()),
            ("success_url", success_url.to_string()),
            ("cancel_url", cancel_url.to_string()),
            ("line_items[0][quantity]", "1".to_string()),
            (
                "line_items[0][price_data][currency]",
                currency.to_lowercase(),
            ),
            (
                "line_items[0][price_data][unit_amount]",
                amount_minor.to_string(),
            ),
            (
                "line_items[0][price_data][product_data][name]",
                product_name.to_string(),
            ),
            ("metadata[reference]", reference.to_string()),
            (
                "payment_intent_data[metadata][reference]",
                reference.to_string(),
            ),
            ("client_reference_id", reference.to_string()),
            ("ui_mode", "hosted".to_string()),
        ];
        let body = fields
            .iter()
            .map(|(k, v)| format!("{}={}", form_encode(k), form_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        HttpRequestSpec {
            method: "POST",
            url: SESSIONS_URL.to_string(),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Bearer {}", self.secret_key),
                ),
                (
                    "Content-Type".to_string(),
                    "application/x-www-form-urlencoded".to_string(),
                ),
            ],
            body,
        }
    }
}

/// Minimal `application/x-www-form-urlencoded` encoder (RFC 1866:
/// alphanumerics and `-_.` pass through, space becomes `+`, the rest
/// percent-encode). Keys contain `[`/`]` which Stripe accepts raw,
/// so brackets are passed through as Stripe's own examples do.
fn form_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'[' | b']' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Parsed create-session response (only the fields we rely on).
#[derive(Debug, Clone, Deserialize)]
pub struct CheckoutSession {
    pub id: String,
    pub url: Option<String>,
    #[serde(default)]
    pub payment_status: Option<String>,
}

/// Parse a create-session response body.
pub fn parse_session(body: &str) -> Result<CheckoutSession, GatewayError> {
    serde_json::from_str(body)
        .map_err(|e| GatewayError::Provider(format!("stripe session parse: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> StripeCheckoutConfig {
        StripeCheckoutConfig::from_lookup(|name| match name {
            "STRIPE_SECRET_KEY" => Some("sk_test_abc123".into()),
            _ => None,
        })
        .unwrap()
    }

    #[test]
    fn missing_key_is_not_configured() {
        let err = StripeCheckoutConfig::from_lookup(|_| None).unwrap_err();
        assert!(matches!(err, GatewayError::NotConfigured(_)));
    }

    #[test]
    fn non_stripe_key_is_invalid_config() {
        let err = StripeCheckoutConfig::from_lookup(|name| match name {
            "STRIPE_SECRET_KEY" => Some("pk_test_publishable".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(matches!(err, GatewayError::InvalidConfig(_)));
    }

    #[test]
    fn live_key_is_detected() {
        let cfg = StripeCheckoutConfig::from_lookup(|name| match name {
            "STRIPE_SECRET_KEY" => Some("sk_live_xyz".into()),
            _ => None,
        })
        .unwrap();
        assert!(cfg.live);
        assert!(!test_config().live);
    }

    #[test]
    fn session_request_shape() {
        let spec = test_config().create_session_request(
            125_000,
            "BDT",
            "Order #42",
            "https://shop.example.com/ok?x=1",
            "https://shop.example.com/cancel",
            "MP-REF-1",
        );
        assert_eq!(spec.method, "POST");
        assert_eq!(spec.url, SESSIONS_URL);
        assert!(spec
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer sk_test_abc123"));
        assert!(spec.body.contains("mode=payment"));
        assert!(spec
            .body
            .contains("line_items[0][price_data][unit_amount]=125000"));
        assert!(spec
            .body
            .contains("line_items[0][price_data][currency]=bdt"));
        assert!(spec.body.contains("client_reference_id=MP-REF-1"));
        // URLs must be form-encoded inside the body.
        assert!(spec
            .body
            .contains("success_url=https%3A%2F%2Fshop.example.com%2Fok%3Fx%3D1"));
    }

    #[test]
    fn parses_session_response() {
        let session = parse_session(
            r#"{"id":"cs_test_123","object":"checkout.session","url":"https://checkout.stripe.com/c/pay/cs_test_123","payment_status":"unpaid"}"#,
        )
        .unwrap();
        assert_eq!(session.id, "cs_test_123");
        assert!(session.url.unwrap().contains("checkout.stripe.com"));
        assert!(parse_session("{}").is_err());
    }
}
