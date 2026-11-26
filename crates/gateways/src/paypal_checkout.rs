//! PayPal Orders v2 — request shaping (Wave 10).
//!
//! Original clean-room implementation of the *shapes* of PayPal's
//! documented flow:
//!
//! 1. **Access token** — `POST /v1/oauth2/token` with HTTP Basic
//!    auth (client id : secret) and `grant_type=client_credentials`.
//! 2. **Create order** — `POST /v2/checkout/orders` with the token,
//!    `intent: "CAPTURE"`, one purchase unit carrying the amount and
//!    the merchant's reference (`custom_id` / `invoice_id`); the
//!    response's `rel: "payer-action"` / `"approve"` link is where
//!    the payer is sent.
//!
//! No HTTP is performed here — see [`crate::bkash_tokenized`] for
//! the pattern. Credentials come from the environment
//! (`PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET`; `PAYPAL_SANDBOX=0`
//! selects live). Missing credentials are
//! [`GatewayError::NotConfigured`] naming them; no approve link is
//! ever fabricated.

use serde::Deserialize;

use crate::bkash_tokenized::HttpRequestSpec;
use crate::types::GatewayError;

/// Documented API base URLs.
pub const SANDBOX_BASE: &str = "https://api-m.sandbox.paypal.com";
/// Live API base URL.
pub const LIVE_BASE: &str = "https://api-m.paypal.com";

/// Credentials + environment for the PayPal flow.
#[derive(Debug, Clone)]
pub struct PayPalConfig {
    pub client_id: String,
    pub client_secret: String,
    pub sandbox: bool,
}

impl PayPalConfig {
    /// Build from an explicit variable lookup (env in production,
    /// fixtures in tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, GatewayError> {
        let mut missing: Vec<&str> = Vec::new();
        let mut take = |name: &'static str| match get(name) {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                missing.push(name);
                String::new()
            }
        };
        let client_id = take("PAYPAL_CLIENT_ID");
        let client_secret = take("PAYPAL_CLIENT_SECRET");
        if !missing.is_empty() {
            return Err(GatewayError::NotConfigured(format!(
                "paypal: missing env {}",
                missing.join(", ")
            )));
        }
        let sandbox = get("PAYPAL_SANDBOX")
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or(true);
        Ok(Self { client_id, client_secret, sandbox })
    }

    /// Build from the process environment.
    pub fn from_env() -> Result<Self, GatewayError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Base URL for the configured environment.
    pub fn base_url(&self) -> &'static str {
        if self.sandbox { SANDBOX_BASE } else { LIVE_BASE }
    }

    /// Shape the OAuth client-credentials token request. The Basic
    /// credential is base64(`client_id:client_secret`).
    pub fn token_request(&self) -> HttpRequestSpec {
        HttpRequestSpec {
            method: "POST",
            url: format!("{}/v1/oauth2/token", self.base_url()),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Basic {}", base64_encode(format!("{}:{}", self.client_id, self.client_secret).as_bytes())),
                ),
                ("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()),
            ],
            body: "grant_type=client_credentials".to_string(),
        }
    }

    /// Shape the create-order request. `amount_major` is a decimal
    /// string in `currency` (PayPal uses major units, e.g. "12.50").
    pub fn create_order_request(
        &self,
        access_token: &str,
        amount_major: &str,
        currency: &str,
        reference: &str,
        return_url: &str,
        cancel_url: &str,
    ) -> HttpRequestSpec {
        HttpRequestSpec {
            method: "POST",
            url: format!("{}/v2/checkout/orders", self.base_url()),
            headers: vec![
                ("Authorization".to_string(), format!("Bearer {access_token}")),
                ("Content-Type".to_string(), "application/json".to_string()),
                // Idempotency: one order per reference even on retries.
                ("PayPal-Request-Id".to_string(), reference.to_string()),
            ],
            body: serde_json::json!({
                "intent": "CAPTURE",
                "purchase_units": [{
                    "reference_id": reference,
                    "custom_id": reference,
                    "amount": { "currency_code": currency, "value": amount_major }
                }],
                "payment_source": {
                    "paypal": {
                        "experience_context": {
                            "return_url": return_url,
                            "cancel_url": cancel_url,
                            "user_action": "PAY_NOW"
                        }
                    }
                }
            })
            .to_string(),
        }
    }
}

/// Minimal base64 encoder (standard alphabet, with padding) — the
/// gateways crate intentionally carries no base64 dependency for
/// this one use.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Parsed access-token response.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

/// One link object from an order response.
#[derive(Debug, Clone, Deserialize)]
pub struct OrderLink {
    pub href: String,
    pub rel: String,
}

/// Parsed create-order response.
#[derive(Debug, Clone, Deserialize)]
pub struct OrderResponse {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub links: Vec<OrderLink>,
}

impl OrderResponse {
    /// The URL to send the payer to (`rel` is `"payer-action"` on
    /// newer responses, `"approve"` on older ones).
    pub fn payer_action_url(&self) -> Option<&str> {
        self.links
            .iter()
            .find(|l| l.rel == "payer-action" || l.rel == "approve")
            .map(|l| l.href.as_str())
    }
}

/// Parse a token response body.
pub fn parse_token(body: &str) -> Result<TokenResponse, GatewayError> {
    serde_json::from_str(body)
        .map_err(|e| GatewayError::Provider(format!("paypal token parse: {e}")))
}

/// Parse a create-order response body.
pub fn parse_order(body: &str) -> Result<OrderResponse, GatewayError> {
    serde_json::from_str(body)
        .map_err(|e| GatewayError::Provider(format!("paypal order parse: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PayPalConfig {
        PayPalConfig::from_lookup(|name| match name {
            "PAYPAL_CLIENT_ID" => Some("cid-123".into()),
            "PAYPAL_CLIENT_SECRET" => Some("sec-456".into()),
            _ => None,
        })
        .unwrap()
    }

    #[test]
    fn missing_credentials_are_named() {
        let err = PayPalConfig::from_lookup(|_| None).unwrap_err();
        match err {
            GatewayError::NotConfigured(msg) => {
                assert!(msg.contains("PAYPAL_CLIENT_ID"), "{msg}");
                assert!(msg.contains("PAYPAL_CLIENT_SECRET"), "{msg}");
            }
            other => panic!("expected NotConfigured, got {other:?}"),
        }
    }

    #[test]
    fn token_request_uses_basic_auth_and_sandbox_by_default() {
        let spec = config().token_request();
        assert!(spec.url.starts_with(SANDBOX_BASE));
        assert!(spec.url.ends_with("/v1/oauth2/token"));
        assert_eq!(spec.body, "grant_type=client_credentials");
        let auth = spec.headers.iter().find(|(k, _)| k == "Authorization").unwrap();
        // base64("cid-123:sec-456")
        assert_eq!(auth.1, "Basic Y2lkLTEyMzpzZWMtNDU2");
    }

    #[test]
    fn create_order_request_shape() {
        let spec = config().create_order_request(
            "tok", "12.50", "USD", "MP-REF-9",
            "https://shop.example.com/ok", "https://shop.example.com/cancel",
        );
        let body: serde_json::Value = serde_json::from_str(&spec.body).unwrap();
        assert_eq!(body["intent"], "CAPTURE");
        assert_eq!(body["purchase_units"][0]["amount"]["value"], "12.50");
        assert_eq!(body["purchase_units"][0]["amount"]["currency_code"], "USD");
        assert_eq!(body["purchase_units"][0]["custom_id"], "MP-REF-9");
        assert!(spec.headers.iter().any(|(k, v)| k == "PayPal-Request-Id" && v == "MP-REF-9"));
    }

    #[test]
    fn parses_order_and_finds_payer_action_link() {
        let order = parse_order(
            r#"{"id":"5O190127TN364715T","status":"PAYER_ACTION_REQUIRED","links":[{"href":"https://www.sandbox.paypal.com/checkoutnow?token=5O190127TN364715T","rel":"payer-action","method":"GET"},{"href":"https://api-m.sandbox.paypal.com/v2/checkout/orders/5O190127TN364715T","rel":"self","method":"GET"}]}"#,
        )
        .unwrap();
        assert_eq!(order.id, "5O190127TN364715T");
        assert!(order.payer_action_url().unwrap().contains("paypal.com/checkoutnow"));
        let token = parse_token(r#"{"access_token":"A21AA-example","token_type":"Bearer","expires_in":32400}"#).unwrap();
        assert_eq!(token.access_token, "A21AA-example");
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
