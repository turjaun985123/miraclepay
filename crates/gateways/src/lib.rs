//! MiraclePay gateway adapters — one module per gateway, all implementing
//! the shared `PaymentGateway` trait (individually callable operations).
pub mod bkash;
pub mod bkash_tokenized;
pub mod easypaisa;
pub mod jazzcash;
pub mod manual;
pub mod nagad;
pub mod paypal;
pub mod paypal_checkout;
pub mod paytm;
pub mod phonepe;
pub mod razorpay;
pub mod registry;
pub mod rocket;
pub mod stripe;
pub mod stripe_checkout;
pub mod surecash;
pub mod traits;
pub mod types;
pub mod upay;
pub mod upi;

pub use traits::PaymentGateway;
pub use types::{GatewayError, GatewayId};
