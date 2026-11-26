//! Live HTTP transport for gateway APIs (Waves 76–84).
//!
//! Waves 7/9/10/30 shaped provider requests as
//! [`HttpRequestSpec`]s and parsers for the responses, but nothing
//! ever sent them. This module is the missing half:
//!
//! - [`HttpTransport`] abstracts one request/response round trip
//!   so flows are testable against a scripted mock (no network in
//!   tests, ever).
//! - [`ReqwestTransport`] is the production implementation
//!   (rustls, 15 s timeout, no redirects followed blindly — the
//!   client default).
//! - The `*_live` flow functions combine config → spec → transport
//!   → parser. Configuration still comes only from the environment
//!   (see each provider's config type); without credentials the
//!   config constructors fail with `NotConfigured` before any
//!   network call, and a provider error response is always a
//!   [`GatewayError::Provider`] — **nothing here can fake a
//!   successful payment**.

use crate::bkash_tokenized::{
    parse_create_payment, parse_grant_token, BkashTokenizedConfig, CreatePaymentResponse,
    GrantTokenResponse, HttpRequestSpec,
};
use crate::paypal_checkout::{parse_order, parse_token, OrderResponse, PayPalConfig};
use crate::razorpay_orders::{parse_order as parse_razorpay_order, RazorpayConfig, RazorpayOrder};
use crate::stripe_checkout::{parse_session, CheckoutSession, StripeCheckoutConfig};
use crate::types::GatewayError;

/// One HTTP round trip: send a shaped request, return
/// `(status, body)`.
pub trait HttpTransport: Send + Sync {
    fn send(&self, spec: &HttpRequestSpec) -> Result<(u16, String), GatewayError>;
}

/// Production transport backed by reqwest (blocking, rustls).
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, GatewayError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| GatewayError::Network(format!("http client: {e}")))?;
        Ok(Self { client })
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new().expect("reqwest client construction")
    }
}

impl HttpTransport for ReqwestTransport {
    fn send(&self, spec: &HttpRequestSpec) -> Result<(u16, String), GatewayError> {
        let mut request = match spec.method {
            "GET" => self.client.get(&spec.url),
            _ => self.client.post(&spec.url),
        };
        for (name, value) in &spec.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        if spec.method != "GET" {
            request = request.body(spec.body.clone());
        }
        let response = request
            .send()
            .map_err(|e| GatewayError::Network(format!("{}: {e}", spec.url)))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .map_err(|e| GatewayError::Network(format!("response body: {e}")))?;
        Ok((status, body))
    }
}

/// Send a spec and require a 2xx, mapping anything else to a
/// provider error that carries the status and a truncated body —
/// never a silent success.
fn execute<T: HttpTransport>(
    transport: &T,
    spec: &HttpRequestSpec,
    provider: &str,
) -> Result<String, GatewayError> {
    let (status, body) = transport.send(spec)?;
    if (200..300).contains(&status) {
        Ok(body)
    } else {
        let excerpt: String = body.chars().take(200).collect();
        Err(GatewayError::Provider(format!(
            "{provider} HTTP {status}: {excerpt}"
        )))
    }
}

// ---------------------------------------------------------------------
// bKash tokenized (Wave 76)
// ---------------------------------------------------------------------

/// Grant a bKash tokenized access token (live HTTP).
pub fn bkash_grant_token<T: HttpTransport>(
    transport: &T,
    config: &BkashTokenizedConfig,
) -> Result<GrantTokenResponse, GatewayError> {
    let body = execute(transport, &config.grant_token_request(), "bkash")?;
    parse_grant_token(&body)
}

/// Create a bKash tokenized payment; returns the payment id and
/// the bKash redirect URL the payer must visit.
#[allow(clippy::too_many_arguments)] // mirrors the provider spec signature
pub fn bkash_create_payment<T: HttpTransport>(
    transport: &T,
    config: &BkashTokenizedConfig,
    access_token: &str,
    callback_url: &str,
    amount_bdt: &str,
    invoice_number: &str,
    payer_reference: &str,
) -> Result<CreatePaymentResponse, GatewayError> {
    let body = execute(
        transport,
        &config.create_payment_request(
            access_token,
            callback_url,
            amount_bdt,
            invoice_number,
            payer_reference,
        ),
        "bkash",
    )?;
    parse_create_payment(&body)
}

/// Execute a bKash payment after the payer returns (Wave 82's
/// callback handler calls this). Returns the raw provider body —
/// the caller checks `transactionStatus` before settling anything.
pub fn bkash_execute_payment<T: HttpTransport>(
    transport: &T,
    config: &BkashTokenizedConfig,
    access_token: &str,
    payment_id: &str,
) -> Result<serde_json::Value, GatewayError> {
    let body = execute(
        transport,
        &config.execute_payment_request(access_token, payment_id),
        "bkash",
    )?;
    serde_json::from_str(&body)
        .map_err(|e| GatewayError::Provider(format!("bkash execute parse: {e}")))
}

// ---------------------------------------------------------------------
// Stripe (Wave 77)
// ---------------------------------------------------------------------

/// Create a live Stripe Checkout Session; the returned session
/// carries the hosted URL.
#[allow(clippy::too_many_arguments)] // mirrors the provider spec signature
pub fn stripe_create_session<T: HttpTransport>(
    transport: &T,
    config: &StripeCheckoutConfig,
    amount_minor: i64,
    currency: &str,
    product_name: &str,
    success_url: &str,
    cancel_url: &str,
    reference: &str,
) -> Result<CheckoutSession, GatewayError> {
    let body = execute(
        transport,
        &config.create_session_request(
            amount_minor,
            currency,
            product_name,
            success_url,
            cancel_url,
            reference,
        ),
        "stripe",
    )?;
    parse_session(&body)
}

// ---------------------------------------------------------------------
// PayPal (Wave 78)
// ---------------------------------------------------------------------

/// Create a live PayPal order (OAuth + Orders v2 in one flow).
#[allow(clippy::too_many_arguments)] // mirrors the provider spec signature
pub fn paypal_create_order<T: HttpTransport>(
    transport: &T,
    config: &PayPalConfig,
    amount_major: &str,
    currency: &str,
    reference: &str,
    return_url: &str,
    cancel_url: &str,
) -> Result<OrderResponse, GatewayError> {
    let token_body = execute(transport, &config.token_request(), "paypal")?;
    let token = parse_token(&token_body)?;
    let body = execute(
        transport,
        &config.create_order_request(
            &token.access_token,
            amount_major,
            currency,
            reference,
            return_url,
            cancel_url,
        ),
        "paypal",
    )?;
    parse_order(&body)
}

// ---------------------------------------------------------------------
// Razorpay (Wave 83)
// ---------------------------------------------------------------------

/// Create a live Razorpay order.
pub fn razorpay_create_order<T: HttpTransport>(
    transport: &T,
    config: &RazorpayConfig,
    amount_minor: i64,
    reference: &str,
) -> Result<RazorpayOrder, GatewayError> {
    let spec = config.create_order_request(amount_minor, reference)?;
    let body = execute(transport, &spec, "razorpay")?;
    parse_razorpay_order(&body)
}

// ---------------------------------------------------------------------
// Nagad (Wave 79)
// ---------------------------------------------------------------------

/// Nagad merchant-API configuration. The base URL has no default
/// on purpose: Nagad issues environment URLs per merchant, and
/// inventing one would send credentials to a guessed host.
/// `NAGAD_API_BASE` + `NAGAD_MERCHANT_ID` are required.
#[derive(Debug, Clone)]
pub struct NagadApiConfig {
    pub base_url: String,
    pub merchant_id: String,
}

impl NagadApiConfig {
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, GatewayError> {
        let base_url = get("NAGAD_API_BASE").filter(|v| !v.trim().is_empty());
        let merchant_id = get("NAGAD_MERCHANT_ID").filter(|v| !v.trim().is_empty());
        match (base_url, merchant_id) {
            (Some(base_url), Some(merchant_id)) => Ok(Self {
                base_url: base_url.trim_end_matches('/').to_string(),
                merchant_id,
            }),
            _ => Err(GatewayError::NotConfigured(
                "NAGAD_API_BASE and NAGAD_MERCHANT_ID are required for the Nagad API".to_string(),
            )),
        }
    }

    pub fn from_env() -> Result<Self, GatewayError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Shape the payment-verification request
    /// (`GET {base}/api/dfs/verify/payment/{payment_ref_id}`).
    pub fn verify_request(&self, payment_ref_id: &str) -> HttpRequestSpec {
        HttpRequestSpec {
            method: "GET",
            url: format!("{}/api/dfs/verify/payment/{payment_ref_id}", self.base_url),
            headers: vec![
                ("X-KM-Api-Version".to_string(), "v-0.2.0".to_string()),
                ("X-KM-MERCHANT-ID".to_string(), self.merchant_id.clone()),
                ("Accept".to_string(), "application/json".to_string()),
            ],
            body: String::new(),
        }
    }
}

/// Verify a Nagad payment by reference id (live HTTP). Returns the
/// provider's raw verdict JSON — the caller settles only on an
/// explicit success status in that payload.
pub fn nagad_verify_payment<T: HttpTransport>(
    transport: &T,
    config: &NagadApiConfig,
    payment_ref_id: &str,
) -> Result<serde_json::Value, GatewayError> {
    let body = execute(transport, &config.verify_request(payment_ref_id), "nagad")?;
    serde_json::from_str(&body)
        .map_err(|e| GatewayError::Provider(format!("nagad verify parse: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Scripted transport: records requests, replays responses.
    struct MockTransport {
        responses: Mutex<Vec<(u16, String)>>,
        requests: Mutex<Vec<HttpRequestSpec>>,
    }

    impl MockTransport {
        fn new(responses: Vec<(u16, String)>) -> Self {
            Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl HttpTransport for MockTransport {
        fn send(&self, spec: &HttpRequestSpec) -> Result<(u16, String), GatewayError> {
            self.requests.lock().unwrap().push(spec.clone());
            Ok(self.responses.lock().unwrap().remove(0))
        }
    }

    fn bkash_config() -> BkashTokenizedConfig {
        BkashTokenizedConfig::from_lookup(|name| match name {
            "BKASH_USERNAME" => Some("sandboxUser".to_string()),
            "BKASH_PASSWORD" => Some("sandboxPass".to_string()),
            "BKASH_APP_KEY" => Some("appkey".to_string()),
            "BKASH_APP_SECRET" => Some("appsecret".to_string()),
            _ => None,
        })
        .expect("config")
    }

    #[test]
    fn bkash_flow_grants_creates_and_executes() {
        let transport = MockTransport::new(vec![
            (
                200,
                r#"{"id_token":"tok123","token_type":"Bearer","expires_in":3600,"refresh_token":"r"}"#
                    .to_string(),
            ),
            (
                200,
                r#"{"paymentID":"TRX001","bkashURL":"https://sandbox.bka.sh/checkout/TRX001","statusCode":"0000"}"#
                    .to_string(),
            ),
            (
                200,
                r#"{"paymentID":"TRX001","transactionStatus":"Completed","trxID":"ABC123"}"#
                    .to_string(),
            ),
        ]);
        let config = bkash_config();
        let token = bkash_grant_token(&transport, &config).expect("grant");
        let created = bkash_create_payment(
            &transport,
            &config,
            &token.id_token,
            "https://shop.example/callback",
            "1250.00",
            "INV-1",
            "01700000000",
        )
        .expect("create");
        assert_eq!(created.payment_id, "TRX001");
        let executed =
            bkash_execute_payment(&transport, &config, &token.id_token, &created.payment_id)
                .expect("execute");
        assert_eq!(executed["transactionStatus"], "Completed");
        assert_eq!(transport.requests.lock().unwrap().len(), 3);
    }

    #[test]
    fn provider_error_is_never_a_success() {
        let transport =
            MockTransport::new(vec![(401, r#"{"message":"unauthorized"}"#.to_string())]);
        let config = bkash_config();
        let err = bkash_grant_token(&transport, &config).unwrap_err();
        assert!(matches!(err, GatewayError::Provider(_)), "{err}");
    }

    #[test]
    fn stripe_session_create_uses_spec_and_parser() {
        let transport = MockTransport::new(vec![(
            200,
            r#"{"id":"cs_test_1","url":"https://checkout.stripe.com/c/pay/cs_test_1","payment_status":"unpaid"}"#
                .to_string(),
        )]);
        let config = StripeCheckoutConfig::from_lookup(|name| match name {
            "STRIPE_SECRET_KEY" => Some("sk_test_123".to_string()),
            _ => None,
        })
        .expect("config");
        let session = stripe_create_session(
            &transport,
            &config,
            5000,
            "usd",
            "Widget",
            "https://shop.example/ok",
            "https://shop.example/cancel",
            "ref-1",
        )
        .expect("session");
        assert_eq!(session.id, "cs_test_1");
        assert!(session.url.unwrap().contains("checkout.stripe.com"));
    }

    #[test]
    fn paypal_order_flow_fetches_token_first() {
        let transport = MockTransport::new(vec![
            (
                200,
                r#"{"access_token":"pp-tok","token_type":"Bearer","expires_in":3600}"#.to_string(),
            ),
            (
                201,
                r#"{"id":"ORDER1","status":"CREATED","links":[{"href":"https://www.paypal.com/checkoutnow?token=ORDER1","rel":"payer-action","method":"GET"}]}"#
                    .to_string(),
            ),
        ]);
        let config = PayPalConfig::from_lookup(|name| match name {
            "PAYPAL_CLIENT_ID" => Some("cid".to_string()),
            "PAYPAL_CLIENT_SECRET" => Some("secret".to_string()),
            _ => None,
        })
        .expect("config");
        let order = paypal_create_order(
            &transport,
            &config,
            "12.50",
            "USD",
            "ref-9",
            "https://shop.example/ok",
            "https://shop.example/cancel",
        )
        .expect("order");
        assert_eq!(order.id, "ORDER1");
        assert!(order.payer_action_url().is_some());
    }

    #[test]
    fn razorpay_order_create() {
        let transport = MockTransport::new(vec![(
            200,
            r#"{"id":"order_123","amount":50000,"currency":"INR","receipt":"ref-2","status":"created"}"#
                .to_string(),
        )]);
        let config = RazorpayConfig::from_lookup(|name| match name {
            "RAZORPAY_KEY_ID" => Some("rzp_test_abc".to_string()),
            "RAZORPAY_KEY_SECRET" => Some("secret".to_string()),
            _ => None,
        })
        .expect("config");
        let order = razorpay_create_order(&transport, &config, 50000, "ref-2").expect("order");
        assert_eq!(order.id, "order_123");
    }

    #[test]
    fn nagad_requires_base_and_merchant() {
        let err = NagadApiConfig::from_lookup(|_| None).unwrap_err();
        assert!(matches!(err, GatewayError::NotConfigured(_)));
        let transport = MockTransport::new(vec![(
            200,
            r#"{"status":"Success","trxId":"71B7A1"}"#.to_string(),
        )]);
        let config = NagadApiConfig::from_lookup(|name| match name {
            "NAGAD_API_BASE" => Some("https://nagad.example".to_string()),
            "NAGAD_MERCHANT_ID" => Some("M123".to_string()),
            _ => None,
        })
        .expect("config");
        let verdict = nagad_verify_payment(&transport, &config, "REF1").expect("verify");
        assert_eq!(verdict["status"], "Success");
    }
}

#[cfg(test)]
mod sandbox_audit_tests {
    use super::*;

    /// Wave 84: with no explicit live opt-in, every live flow must
    /// target the provider's sandbox — never production.
    #[test]
    fn default_configs_target_sandbox_hosts() {
        let bkash = bkash_config_for_audit();
        assert!(bkash.grant_token_request().url.contains("sandbox.bka.sh"));
        let paypal = PayPalConfig::from_lookup(|name| match name {
            "PAYPAL_CLIENT_ID" => Some("cid".to_string()),
            "PAYPAL_CLIENT_SECRET" => Some("secret".to_string()),
            _ => None,
        })
        .expect("paypal config");
        assert!(paypal.token_request().url.contains("sandbox.paypal.com"));
    }

    fn bkash_config_for_audit() -> BkashTokenizedConfig {
        BkashTokenizedConfig::from_lookup(|name| match name {
            "BKASH_USERNAME" => Some("u".to_string()),
            "BKASH_PASSWORD" => Some("p".to_string()),
            "BKASH_APP_KEY" => Some("k".to_string()),
            "BKASH_APP_SECRET" => Some("s".to_string()),
            _ => None,
        })
        .expect("bkash config")
    }
}
