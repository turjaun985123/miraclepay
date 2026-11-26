//! Wave 49: money literals in this codebase are written in
//! major_minor grouping (e.g. `1_500_00` = 1500.00) on purpose —
//! it mirrors how amounts are spoken. That trips clippy's
//! inconsistent_digit_grouping, which expects 3-digit groups;
//! the convention is deliberate, so the lint is allowed crate-wide.
#![allow(clippy::inconsistent_digit_grouping)]

//! MiraclePay gateway adapters — one module per gateway, all implementing
//! the shared `PaymentGateway` trait (individually callable operations).
pub mod bkash;
pub mod bkash_tokenized;
pub mod easypaisa;
pub mod in_manual;
pub mod jazzcash;
pub mod manual;
pub mod nagad;
pub mod paypal;
pub mod paypal_checkout;
pub mod paytm;
pub mod phonepe;
pub mod pk_manual;
pub mod razorpay;
pub mod razorpay_orders;
pub mod registry;
pub mod rocket;
pub mod stripe;
pub mod stripe_checkout;
pub mod surecash;
pub mod traits;
pub mod transport;
pub mod types;
pub mod upay;
pub mod upi;

pub use traits::PaymentGateway;
pub use types::{GatewayError, GatewayId};
