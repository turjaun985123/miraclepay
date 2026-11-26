//! MiraclePay domain types — framework-free, reusable single-responsibility modules.
pub mod audit;
pub mod errors;
pub mod ids;
pub mod fx;
pub mod invoice;
pub mod kyc;
pub mod invoice_pdf;
pub mod merchant;
pub mod money;
pub mod notification;
pub mod payment_link;
pub mod refund;
pub mod settings;
pub mod settlement;
pub mod stats;
pub mod subscription;
pub mod totp;
pub mod transaction;
pub mod user;
pub mod analytics;
pub mod api_key;
pub mod password;
pub mod webhook;

pub use money::{Currency, Money};
