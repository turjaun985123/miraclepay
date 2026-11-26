//! Razorpay Orders API — request specification (Wave 30).
//!
//! Razorpay payments start by creating an **Order** server-side
//! (`POST https://api.razorpay.com/v1/orders`, HTTP Basic auth with
//! the merchant's key id + secret); the checkout then needs the
//! returned order id. The pre-Wave-30 adapter skipped this and
//! handed customers a fabricated `checkout.js?order_ref=…` URL that
//! could never take a payment — that URL is gone.
//!
//! Like the other Wave 7/9/10 modules, this builds the exact
//! request and parses the response, with **no HTTP performed** —
//! the transport lands with live credentials. Config comes from
//! `RAZORPAY_KEY_ID` / `RAZORPAY_KEY_SECRET` env vars; key ids must
//! carry the `rzp_test_` / `rzp_live_` prefix.

use serde::Deserialize;

use crate::bkash_tokenized::HttpRequestSpec;
use crate::paypal_checkout::base64_encode;
use crate::types::GatewayError;

/// Razorpay Orders endpoint.
pub const ORDERS_URL: &str = "https://api.razorpay.com/v1/orders";

/// Configuration for the Razorpay orders flow.
#[derive(Debug, Clone)]
pub struct RazorpayConfig {
    pub key_id: String,
    pub key_secret: String,
    pub live: bool,
}

impl RazorpayConfig {
    /// Build from an explicit variable lookup (env in production,
    /// fixtures in tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, GatewayError> {
        let key_id = match get("RAZORPAY_KEY_ID") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => {
                return Err(GatewayError::NotConfigured(
                    "razorpay: missing env RAZORPAY_KEY_ID".to_string(),
                ))
            }
        };
        let key_secret = match get("RAZORPAY_KEY_SECRET") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => {
                return Err(GatewayError::NotConfigured(
                    "razorpay: missing env RAZORPAY_KEY_SECRET".to_string(),
                ))
            }
        };
        let live = if key_id.starts_with("rzp_live_") {
            true
        } else if key_id.starts_with("rzp_test_") {
            false
        } else {
            return Err(GatewayError::InvalidConfig(
                "razorpay: RAZORPAY_KEY_ID must start with rzp_test_ or rzp_live_".into(),
            ));
        };
        Ok(Self {
            key_id,
            key_secret,
            live,
        })
    }

    /// Build from the process environment.
    pub fn from_env() -> Result<Self, GatewayError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Shape the create-order request. `amount_minor` is in paise,
    /// exactly as Razorpay expects; the MiraclePay reference rides
    /// as the order `receipt` (≤ 40 chars per Razorpay's docs) and
    /// in `notes` untruncated.
    pub fn create_order_request(
        &self,
        amount_minor: i64,
        reference: &str,
    ) -> Result<HttpRequestSpec, GatewayError> {
        if amount_minor <= 0 {
            return Err(GatewayError::Provider(
                "razorpay: amount must be positive".into(),
            ));
        }
        let receipt: String = reference.chars().take(40).collect();
        let body = serde_json::json!({
            "amount": amount_minor,
            "currency": "INR",
            "receipt": receipt,
            "notes": { "reference": reference },
        })
        .to_string();
        Ok(HttpRequestSpec {
            method: "POST",
            url: ORDERS_URL.to_string(),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!(
                        "Basic {}",
                        base64_encode(format!("{}:{}", self.key_id, self.key_secret).as_bytes())
                    ),
                ),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body,
        })
    }
}

/// Parsed create-order response (fields we rely on).
#[derive(Debug, Clone, Deserialize)]
pub struct RazorpayOrder {
    pub id: String,
    pub amount: i64,
    pub currency: String,
    pub status: String,
}

/// Parse a create-order response body.
pub fn parse_order(body: &str) -> Result<RazorpayOrder, GatewayError> {
    serde_json::from_str(body)
        .map_err(|e| GatewayError::Provider(format!("razorpay order parse: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    fn test_config() -> RazorpayConfig {
        RazorpayConfig::from_lookup(lookup(&[
            ("RAZORPAY_KEY_ID", "rzp_test_abc123"),
            ("RAZORPAY_KEY_SECRET", "testsecret"),
        ]))
        .unwrap()
    }

    #[test]
    fn missing_env_is_not_configured() {
        let err = RazorpayConfig::from_lookup(lookup(&[])).unwrap_err();
        assert!(matches!(err, GatewayError::NotConfigured(_)));
    }

    #[test]
    fn bad_key_prefix_is_invalid_config() {
        let err = RazorpayConfig::from_lookup(lookup(&[
            ("RAZORPAY_KEY_ID", "key_abc"),
            ("RAZORPAY_KEY_SECRET", "s"),
        ]))
        .unwrap_err();
        assert!(matches!(err, GatewayError::InvalidConfig(_)));
    }

    #[test]
    fn create_order_request_shape() {
        let spec = test_config()
            .create_order_request(50_000, "MP-REF-1")
            .unwrap();
        assert_eq!(spec.method, "POST");
        assert_eq!(spec.url, ORDERS_URL);
        let auth = &spec.headers[0].1;
        assert!(auth.starts_with("Basic "));
        let body: serde_json::Value = serde_json::from_str(&spec.body).unwrap();
        assert_eq!(body["amount"], 50_000);
        assert_eq!(body["currency"], "INR");
        assert_eq!(body["receipt"], "MP-REF-1");
        assert_eq!(body["notes"]["reference"], "MP-REF-1");
    }

    #[test]
    fn parse_order_response() {
        let order =
            parse_order(r#"{"id":"order_ABC","amount":50000,"currency":"INR","status":"created"}"#)
                .unwrap();
        assert_eq!(order.id, "order_ABC");
        assert_eq!(order.status, "created");
    }
}
