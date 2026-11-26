//! Wave 49: money literals in this codebase are written in
//! major_minor grouping (e.g. `1_500_00` = 1500.00) on purpose —
//! it mirrors how amounts are spoken. That trips clippy's
//! inconsistent_digit_grouping, which expects 3-digit groups;
//! the convention is deliberate, so the lint is allowed crate-wide.
#![allow(clippy::inconsistent_digit_grouping)]

//! MiraclePay domain types — framework-free, reusable single-responsibility modules.
pub mod analytics;
pub mod api_key;
pub mod audit;
pub mod dispute;
pub mod errors;
pub mod fraud;
pub mod fx;
pub mod ids;
pub mod invoice;
pub mod invoice_pdf;
pub mod kyc;
pub mod merchant;
pub mod money;
pub mod notification;
pub mod password;
pub mod payment_link;
pub mod refund;
pub mod settings;
pub mod settlement;
pub mod stats;
pub mod subscription;
pub mod totp;
pub mod transaction;
pub mod user;
pub mod webhook;

pub use money::{Currency, Money};
