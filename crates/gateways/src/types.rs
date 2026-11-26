//! Shared types for MiraclePay payment gateways.
//!
//! Original clean-room implementation for MiraclePay v0.0.1.
//!
//! This module defines the vocabulary every gateway adapter in this
//! crate speaks: which gateway a payment belongs to ([`GatewayId`]),
//! how a gateway is configured for a merchant ([`GatewayConfig`]),
//! what a caller asks for when starting a payment
//! ([`PaymentRequest`]) and what an adapter returns
//! ([`InitiatedPayment`], [`VerifyResult`], [`RefundResult`]), plus
//! the single error type ([`GatewayError`]) adapters report failures
//! with. Keeping these types in one place lets adapters stay small,
//! single-responsibility units that are interchangeable behind a
//! common interface.

use serde::{Deserialize, Serialize};

use miraclepay_domain::money::Money;

/// A payment gateway supported by MiraclePay.
///
/// The variants cover the mobile-money and card/wallet gateways of
/// the three launch markets — Bangladesh, India, and Pakistan — plus
/// the global card processors and a manual (offline / hand-verified)
/// channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayId {
    /// bKash mobile money (Bangladesh).
    Bkash,
    /// Nagad mobile money (Bangladesh).
    Nagad,
    /// Rocket mobile banking by Dutch-Bangla Bank (Bangladesh).
    Rocket,
    /// Upay mobile money (Bangladesh).
    Upay,
    /// SureCash mobile money (Bangladesh).
    Surecash,
    /// Unified Payments Interface bank transfer (India).
    Upi,
    /// Paytm wallet / payments (India).
    Paytm,
    /// PhonePe wallet / UPI (India).
    Phonepe,
    /// Razorpay payment gateway (India).
    Razorpay,
    /// Easypaisa mobile money (Pakistan).
    Easypaisa,
    /// JazzCash mobile money (Pakistan).
    Jazzcash,
    /// Stripe card payments (global).
    Stripe,
    /// PayPal payments (global).
    Paypal,
    /// Manual payment: the customer pays out of band and a human
    /// verifies the transfer before the order is settled.
    Manual,
}

impl GatewayId {
    /// The stable machine-readable identifier of this gateway, e.g.
    /// `"bkash"`.
    ///
    /// This is the value stored in databases, used in API payloads
    /// and configuration files, and accepted by
    /// [`GatewayId::from_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            GatewayId::Bkash => "bkash",
            GatewayId::Nagad => "nagad",
            GatewayId::Rocket => "rocket",
            GatewayId::Upay => "upay",
            GatewayId::Surecash => "surecash",
            GatewayId::Upi => "upi",
            GatewayId::Paytm => "paytm",
            GatewayId::Phonepe => "phonepe",
            GatewayId::Razorpay => "razorpay",
            GatewayId::Easypaisa => "easypaisa",
            GatewayId::Jazzcash => "jazzcash",
            GatewayId::Stripe => "stripe",
            GatewayId::Paypal => "paypal",
            GatewayId::Manual => "manual",
        }
    }

    /// The human-readable brand name of this gateway, e.g. `"bKash"`,
    /// suitable for checkout pages and admin screens.
    pub fn display_name(&self) -> &'static str {
        match self {
            GatewayId::Bkash => "bKash",
            GatewayId::Nagad => "Nagad",
            GatewayId::Rocket => "Rocket",
            GatewayId::Upay => "Upay",
            GatewayId::Surecash => "SureCash",
            GatewayId::Upi => "UPI",
            GatewayId::Paytm => "Paytm",
            GatewayId::Phonepe => "PhonePe",
            GatewayId::Razorpay => "Razorpay",
            GatewayId::Easypaisa => "Easypaisa",
            GatewayId::Jazzcash => "JazzCash",
            GatewayId::Stripe => "Stripe",
            GatewayId::Paypal => "PayPal",
            GatewayId::Manual => "Manual",
        }
    }

    /// The primary country this gateway operates in, as an ISO 3166-1
    /// alpha-2 code: `"BD"` for Bangladesh, `"IN"` for India, `"PK"`
    /// for Pakistan, or `"GLOBAL"` for gateways that are not tied to
    /// a single country.
    pub fn country(&self) -> &'static str {
        match self {
            GatewayId::Bkash
            | GatewayId::Nagad
            | GatewayId::Rocket
            | GatewayId::Upay
            | GatewayId::Surecash => "BD",
            GatewayId::Upi
            | GatewayId::Paytm
            | GatewayId::Phonepe
            | GatewayId::Razorpay => "IN",
            GatewayId::Easypaisa | GatewayId::Jazzcash => "PK",
            GatewayId::Stripe | GatewayId::Paypal | GatewayId::Manual => "GLOBAL",
        }
    }

    /// Look up a gateway by its identifier.
    ///
    /// Matching is case-insensitive (so `"bkash"`, `"bKash"`, and
    /// `"BKASH"` all resolve to [`GatewayId::Bkash`]) and surrounding
    /// whitespace is ignored. Returns `None` for an identifier the
    /// platform does not support.
    pub fn from_str(s: &str) -> Option<GatewayId> {
        match s.trim().to_ascii_lowercase().as_str() {
            "bkash" => Some(GatewayId::Bkash),
            "nagad" => Some(GatewayId::Nagad),
            "rocket" => Some(GatewayId::Rocket),
            "upay" => Some(GatewayId::Upay),
            "surecash" => Some(GatewayId::Surecash),
            "upi" => Some(GatewayId::Upi),
            "paytm" => Some(GatewayId::Paytm),
            "phonepe" => Some(GatewayId::Phonepe),
            "razorpay" => Some(GatewayId::Razorpay),
            "easypaisa" => Some(GatewayId::Easypaisa),
            "jazzcash" => Some(GatewayId::Jazzcash),
            "stripe" => Some(GatewayId::Stripe),
            "paypal" => Some(GatewayId::Paypal),
            "manual" => Some(GatewayId::Manual),
            _ => None,
        }
    }

    /// All gateways supported by the platform, in declaration order.
    pub fn all() -> &'static [GatewayId] {
        &[
            GatewayId::Bkash,
            GatewayId::Nagad,
            GatewayId::Rocket,
            GatewayId::Upay,
            GatewayId::Surecash,
            GatewayId::Upi,
            GatewayId::Paytm,
            GatewayId::Phonepe,
            GatewayId::Razorpay,
            GatewayId::Easypaisa,
            GatewayId::Jazzcash,
            GatewayId::Stripe,
            GatewayId::Paypal,
            GatewayId::Manual,
        ]
    }
}

impl std::fmt::Display for GatewayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configuration for one gateway, as stored for a merchant.
///
/// Only the fields a given gateway actually needs are populated; the
/// rest stay `None`. Gateway-specific settings that have no dedicated
/// field here belong in [`GatewayConfig::extra`] so this struct does
/// not grow a new field for every provider quirk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Which gateway this configuration belongs to.
    pub gateway: GatewayId,
    /// Merchant / agent account number at the provider, when the
    /// gateway identifies merchants by a phone-style number (typical
    /// for mobile-money gateways).
    pub merchant_number: Option<String>,
    /// API key / public credential issued by the provider.
    pub api_key: Option<String>,
    /// API secret / private credential issued by the provider.
    pub api_secret: Option<String>,
    /// Gateway-specific extra settings, as free-form JSON.
    #[serde(default)]
    pub extra: serde_json::Value,
    /// Whether calls should go to the provider's sandbox environment
    /// instead of production.
    pub sandbox: bool,
    /// Whether this gateway is enabled for the merchant. A disabled
    /// gateway must not be offered at checkout or used for new
    /// payments.
    pub enabled: bool,
}

/// A request to start a payment through a gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequest {
    /// The merchant the payment is collected for.
    pub merchant_id: String,
    /// The amount to collect, in an explicit currency.
    pub amount: Money,
    /// The caller's own reference for this payment (for example an
    /// order or invoice identifier). Adapters pass it through to the
    /// provider where supported so payments can be reconciled.
    pub reference: String,
    /// Customer's name, when known.
    pub customer_name: Option<String>,
    /// Customer's email address, when known.
    pub customer_email: Option<String>,
    /// Customer's phone number, when known. Required by most
    /// mobile-money gateways.
    pub customer_phone: Option<String>,
    /// URL the customer or the provider should return to after the
    /// payment attempt, when the gateway supports one.
    pub callback_url: Option<String>,
    /// Free-form JSON metadata carried with the payment.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// The result of successfully initiating a payment.
///
/// Depending on the gateway, the customer continues either by being
/// sent to [`InitiatedPayment::redirect_url`] or by following
/// [`InitiatedPayment::instructions`] (for example dialling a USSD
/// code or sending money to a merchant number).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiatedPayment {
    /// The provider's reference for this payment attempt. Keep it:
    /// verification and refunds are addressed by this value.
    pub provider_ref: String,
    /// URL to redirect the customer to in order to pay, when the
    /// gateway is redirect-based.
    pub redirect_url: Option<String>,
    /// Human-readable instructions for completing the payment, when
    /// the gateway is instruction-based rather than redirect-based.
    pub instructions: Option<String>,
    /// When this payment attempt expires, if the provider enforces a
    /// deadline.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The result of verifying a payment with a gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    /// The transaction identifier assigned by the provider.
    pub provider_trx_id: String,
    /// The amount the provider confirms was paid, in the currency's
    /// minor unit. Callers must compare this against the expected
    /// amount rather than trusting the request alone.
    pub amount_minor: i64,
    /// Who sent the payment (account number, wallet, or similar
    /// provider identifier), when the provider reports it.
    pub sender: Option<String>,
    /// Whether the provider confirms the payment as genuine and
    /// completed.
    pub verified: bool,
}

/// A request to refund a previously verified payment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundRequest {
    /// The provider reference of the original payment, as returned in
    /// [`InitiatedPayment::provider_ref`].
    pub provider_ref: String,
    /// The amount to refund, in the currency's minor unit. May be
    /// less than the original payment for a partial refund, where the
    /// gateway supports one.
    pub amount_minor: i64,
    /// Optional human-readable reason for the refund.
    pub reason: Option<String>,
}

/// The result of a refund request accepted by a gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundResult {
    /// The provider's identifier for the refund itself.
    pub provider_refund_id: String,
    /// The provider-reported refund status (for example `"pending"`,
    /// `"processing"`, or `"completed"`), kept as the provider's own
    /// string so no status value is silently dropped.
    pub status: String,
}

/// Errors a gateway adapter can return.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// The gateway is not configured (or not enabled) for this
    /// merchant, so the operation cannot even be attempted.
    #[error("gateway not configured: {0}")]
    NotConfigured(String),

    /// The stored gateway configuration is present but invalid.
    #[error("invalid gateway configuration: {0}")]
    InvalidConfig(String),

    /// The provider was reached but rejected the operation or
    /// returned an error.
    #[error("provider error: {0}")]
    Provider(String),

    /// This gateway does not support the requested operation.
    #[error("unsupported operation: {0}")]
    Unsupported(String),

    /// A network-level failure while talking to the provider.
    #[error("network error: {0}")]
    Network(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_id_as_str_roundtrips_through_from_str() {
        for gateway in GatewayId::all() {
            assert_eq!(GatewayId::from_str(gateway.as_str()), Some(*gateway));
        }
    }

    #[test]
    fn gateway_id_from_str_is_case_insensitive() {
        assert_eq!(GatewayId::from_str("BKASH"), Some(GatewayId::Bkash));
        assert_eq!(GatewayId::from_str("bKaSh"), Some(GatewayId::Bkash));
        assert_eq!(GatewayId::from_str("Stripe"), Some(GatewayId::Stripe));
        assert_eq!(GatewayId::from_str("PAYPAL"), Some(GatewayId::Paypal));
        assert_eq!(GatewayId::from_str(" jazzcash "), Some(GatewayId::Jazzcash));
    }

    #[test]
    fn gateway_id_from_str_rejects_unknown_identifiers() {
        assert_eq!(GatewayId::from_str(""), None);
        assert_eq!(GatewayId::from_str("unknown"), None);
        assert_eq!(GatewayId::from_str("bkash2"), None);
    }

    #[test]
    fn gateway_id_all_covers_every_variant_once() {
        let all = GatewayId::all();
        assert_eq!(all.len(), 14);
        assert_eq!(all.first(), Some(&GatewayId::Bkash));
        assert_eq!(all.last(), Some(&GatewayId::Manual));
        // No duplicates: every entry roundtrips to a distinct as_str.
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.as_str(), b.as_str());
            }
        }
    }

    #[test]
    fn gateway_id_known_as_str_and_display_name_values() {
        assert_eq!(GatewayId::Bkash.as_str(), "bkash");
        assert_eq!(GatewayId::Bkash.display_name(), "bKash");
        assert_eq!(GatewayId::Surecash.as_str(), "surecash");
        assert_eq!(GatewayId::Surecash.display_name(), "SureCash");
        assert_eq!(GatewayId::Upi.as_str(), "upi");
        assert_eq!(GatewayId::Upi.display_name(), "UPI");
        assert_eq!(GatewayId::Phonepe.as_str(), "phonepe");
        assert_eq!(GatewayId::Phonepe.display_name(), "PhonePe");
        assert_eq!(GatewayId::Paypal.as_str(), "paypal");
        assert_eq!(GatewayId::Paypal.display_name(), "PayPal");
        assert_eq!(GatewayId::Manual.as_str(), "manual");
    }

    #[test]
    fn gateway_id_country_groupings() {
        for gateway in [
            GatewayId::Bkash,
            GatewayId::Nagad,
            GatewayId::Rocket,
            GatewayId::Upay,
            GatewayId::Surecash,
        ] {
            assert_eq!(gateway.country(), "BD", "{gateway:?}");
        }
        for gateway in [
            GatewayId::Upi,
            GatewayId::Paytm,
            GatewayId::Phonepe,
            GatewayId::Razorpay,
        ] {
            assert_eq!(gateway.country(), "IN", "{gateway:?}");
        }
        for gateway in [GatewayId::Easypaisa, GatewayId::Jazzcash] {
            assert_eq!(gateway.country(), "PK", "{gateway:?}");
        }
        for gateway in [GatewayId::Stripe, GatewayId::Paypal, GatewayId::Manual] {
            assert_eq!(gateway.country(), "GLOBAL", "{gateway:?}");
        }
    }

    #[test]
    fn gateway_id_every_gateway_has_a_known_country() {
        for gateway in GatewayId::all() {
            assert!(
                matches!(gateway.country(), "BD" | "IN" | "PK" | "GLOBAL"),
                "{gateway:?} has unexpected country {:?}",
                gateway.country()
            );
        }
    }
}
