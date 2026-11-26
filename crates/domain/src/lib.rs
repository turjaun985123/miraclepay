//! MiraclePay domain types — framework-free, reusable single-responsibility modules.
pub mod audit;
pub mod errors;
pub mod ids;
pub mod invoice;
pub mod merchant;
pub mod money;
pub mod notification;
pub mod payment_link;
pub mod refund;
pub mod settings;
pub mod settlement;
pub mod stats;
pub mod subscription;
pub mod transaction;
pub mod user;
pub mod api_key;
pub mod webhook;

pub use money::{Currency, Money};
