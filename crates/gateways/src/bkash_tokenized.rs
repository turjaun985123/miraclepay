//! bKash Tokenized Checkout — request shaping (Wave 7).
//!
//! Original clean-room implementation of the *shapes* of bKash's
//! documented tokenized checkout flow:
//!
//! 1. **Grant token** — `POST /tokenized/checkout/token/grant` with
//!    the merchant's username/password in headers and app key/secret
//!    in the JSON body.
//! 2. **Create payment** — `POST /tokenized/checkout/create` with the
//!    granted token; returns a `bkashURL` to redirect the payer to.
//! 3. **Execute payment** — `POST /tokenized/checkout/execute` with
//!    the `paymentID` after the payer returns.
//!
//! This module performs **no HTTP**: it builds [`HttpRequestSpec`]s
//! and parses response bodies, so the whole flow is unit-testable
//! without network or credentials (same pattern as the worker's
//! transport injection). A caller with a real HTTP client executes
//! the specs in order.
//!
//! Credentials come from the environment (`BKASH_USERNAME`,
//! `BKASH_PASSWORD`, `BKASH_APP_KEY`, `BKASH_APP_SECRET`;
//! `BKASH_SANDBOX=0` selects the live base URL). When any credential
//! is missing, [`BkashTokenizedConfig::from_env`] fails with
//! [`GatewayError::NotConfigured`] naming the missing variables —
//! it never fabricates credentials or a checkout URL.

use serde::Deserialize;

use crate::types::GatewayError;

/// Documented base URLs for the tokenized checkout API.
pub const SANDBOX_BASE: &str = "https://tokenized.sandbox.bka.sh/v1.2.0-beta";
/// Live (production) base URL.
pub const LIVE_BASE: &str = "https://tokenized.pay.bka.sh/v1.2.0-beta";

/// A fully-shaped HTTP request, ready for any HTTP client.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequestSpec {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// Serialised JSON body.
    pub body: String,
}

/// Credentials + environment for the tokenized checkout flow.
#[derive(Debug, Clone)]
pub struct BkashTokenizedConfig {
    pub username: String,
    pub password: String,
    pub app_key: String,
    pub app_secret: String,
    pub sandbox: bool,
}

impl BkashTokenizedConfig {
    /// Build from an explicit variable lookup (used by
    /// [`Self::from_env`] and by tests). `get` returns the value of
    /// a variable by name.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, GatewayError> {
        let mut missing: Vec<&str> = Vec::new();
        let mut take = |name: &'static str| match get(name) {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                missing.push(name);
                String::new()
            }
        };
        let username = take("BKASH_USERNAME");
        let password = take("BKASH_PASSWORD");
        let app_key = take("BKASH_APP_KEY");
        let app_secret = take("BKASH_APP_SECRET");
        if !missing.is_empty() {
            return Err(GatewayError::NotConfigured(format!(
                "bkash tokenized: missing env {}",
                missing.join(", ")
            )));
        }
        let sandbox = get("BKASH_SANDBOX")
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or(true);
        Ok(Self {
            username,
            password,
            app_key,
            app_secret,
            sandbox,
        })
    }

    /// Build from the process environment.
    pub fn from_env() -> Result<Self, GatewayError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Base URL for the configured environment.
    pub fn base_url(&self) -> &'static str {
        if self.sandbox {
            SANDBOX_BASE
        } else {
            LIVE_BASE
        }
    }

    /// Shape the grant-token request.
    pub fn grant_token_request(&self) -> HttpRequestSpec {
        HttpRequestSpec {
            method: "POST",
            url: format!("{}/tokenized/checkout/token/grant", self.base_url()),
            headers: vec![
                ("username".to_string(), self.username.clone()),
                ("password".to_string(), self.password.clone()),
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Accept".to_string(), "application/json".to_string()),
            ],
            body: serde_json::json!({
                "app_key": self.app_key,
                "app_secret": self.app_secret,
            })
            .to_string(),
        }
    }

    /// Shape the create-payment request. `amount_bdt` is the amount
    /// in taka (major units) as bKash expects a decimal string;
    /// `invoice_number` is the merchant's own reference.
    pub fn create_payment_request(
        &self,
        access_token: &str,
        callback_url: &str,
        amount_bdt: &str,
        invoice_number: &str,
        payer_reference: &str,
    ) -> HttpRequestSpec {
        HttpRequestSpec {
            method: "POST",
            url: format!("{}/tokenized/checkout/create", self.base_url()),
            headers: vec![
                ("Authorization".to_string(), access_token.to_string()),
                ("X-APP-Key".to_string(), self.app_key.clone()),
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Accept".to_string(), "application/json".to_string()),
            ],
            body: serde_json::json!({
                "mode": "0011",
                "payerReference": payer_reference,
                "callbackURL": callback_url,
                "amount": amount_bdt,
                "currency": "BDT",
                "intent": "sale",
                "merchantInvoiceNumber": invoice_number,
            })
            .to_string(),
        }
    }

    /// Shape the execute-payment request for a returned `paymentID`.
    pub fn execute_payment_request(&self, access_token: &str, payment_id: &str) -> HttpRequestSpec {
        HttpRequestSpec {
            method: "POST",
            url: format!("{}/tokenized/checkout/execute", self.base_url()),
            headers: vec![
                ("Authorization".to_string(), access_token.to_string()),
                ("X-APP-Key".to_string(), self.app_key.clone()),
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Accept".to_string(), "application/json".to_string()),
            ],
            body: serde_json::json!({ "paymentID": payment_id }).to_string(),
        }
    }
}

/// Parsed grant-token response.
#[derive(Debug, Clone, Deserialize)]
pub struct GrantTokenResponse {
    #[serde(rename = "id_token")]
    pub id_token: String,
    #[serde(rename = "expires_in", default)]
    pub expires_in: Option<i64>,
    #[serde(rename = "statusCode", default)]
    pub status_code: Option<String>,
}

/// Parsed create-payment response.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePaymentResponse {
    #[serde(rename = "paymentID")]
    pub payment_id: String,
    #[serde(rename = "bkashURL")]
    pub bkash_url: String,
    #[serde(rename = "statusCode", default)]
    pub status_code: Option<String>,
}

/// Parse a grant-token response body.
pub fn parse_grant_token(body: &str) -> Result<GrantTokenResponse, GatewayError> {
    serde_json::from_str(body)
        .map_err(|e| GatewayError::Provider(format!("bkash grant-token parse: {e}")))
}

/// Parse a create-payment response body.
pub fn parse_create_payment(body: &str) -> Result<CreatePaymentResponse, GatewayError> {
    serde_json::from_str(body)
        .map_err(|e| GatewayError::Provider(format!("bkash create-payment parse: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> BkashTokenizedConfig {
        BkashTokenizedConfig::from_lookup(|name| match name {
            "BKASH_USERNAME" => Some("merchant@example.com".into()),
            "BKASH_PASSWORD" => Some("pw".into()),
            "BKASH_APP_KEY" => Some("appkey123".into()),
            "BKASH_APP_SECRET" => Some("secret456".into()),
            _ => None,
        })
        .unwrap()
    }

    #[test]
    fn missing_credentials_are_not_configured_and_named() {
        let err = BkashTokenizedConfig::from_lookup(|_| None).unwrap_err();
        match err {
            GatewayError::NotConfigured(msg) => {
                assert!(msg.contains("BKASH_USERNAME"), "{msg}");
                assert!(msg.contains("BKASH_APP_SECRET"), "{msg}");
            }
            other => panic!("expected NotConfigured, got {other:?}"),
        }
    }

    #[test]
    fn sandbox_is_default_and_live_is_opt_in() {
        assert!(config().sandbox);
        let live = BkashTokenizedConfig::from_lookup(|name| match name {
            "BKASH_SANDBOX" => Some("0".into()),
            "BKASH_USERNAME" => Some("u".into()),
            "BKASH_PASSWORD" => Some("p".into()),
            "BKASH_APP_KEY" => Some("k".into()),
            "BKASH_APP_SECRET" => Some("s".into()),
            _ => None,
        })
        .unwrap();
        assert!(!live.sandbox);
        assert_eq!(live.base_url(), LIVE_BASE);
    }

    #[test]
    fn grant_token_request_shape() {
        let spec = config().grant_token_request();
        assert_eq!(spec.method, "POST");
        assert!(spec.url.starts_with(SANDBOX_BASE));
        assert!(spec.url.ends_with("/tokenized/checkout/token/grant"));
        let body: serde_json::Value = serde_json::from_str(&spec.body).unwrap();
        assert_eq!(body["app_key"], "appkey123");
        assert_eq!(body["app_secret"], "secret456");
        assert!(spec
            .headers
            .iter()
            .any(|(k, v)| k == "username" && v == "merchant@example.com"));
    }

    #[test]
    fn create_payment_request_shape() {
        let spec = config().create_payment_request(
            "tok123",
            "https://shop.example.com/callback",
            "1250.00",
            "INV-42",
            "01700000000",
        );
        let body: serde_json::Value = serde_json::from_str(&spec.body).unwrap();
        assert_eq!(body["mode"], "0011");
        assert_eq!(body["amount"], "1250.00");
        assert_eq!(body["currency"], "BDT");
        assert_eq!(body["intent"], "sale");
        assert_eq!(body["merchantInvoiceNumber"], "INV-42");
        assert!(spec
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "tok123"));
        assert!(spec
            .headers
            .iter()
            .any(|(k, v)| k == "X-APP-Key" && v == "appkey123"));
    }

    #[test]
    fn parses_documented_responses() {
        let grant = parse_grant_token(
            r#"{"statusCode":"0000","statusMessage":"Successful","id_token":"tok-abc","token_type":"Bearer","expires_in":3600,"refresh_token":"r1"}"#,
        )
        .unwrap();
        assert_eq!(grant.id_token, "tok-abc");
        let create = parse_create_payment(
            r#"{"statusCode":"0000","paymentID":"TR0011abc","bkashURL":"https://sandbox.payment.bkash.com/x","callbackURL":"https://shop.example.com/callback","amount":"1250.00","currency":"BDT","intent":"sale","merchantInvoiceNumber":"INV-42","transactionStatus":"Initiated"}"#,
        )
        .unwrap();
        assert_eq!(create.payment_id, "TR0011abc");
        assert!(create.bkash_url.contains("bkash.com"));
    }

    #[test]
    fn malformed_responses_are_provider_errors() {
        assert!(parse_grant_token("{}").is_err());
        assert!(parse_create_payment("not json").is_err());
    }
}
