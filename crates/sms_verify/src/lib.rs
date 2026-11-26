//! Wave 49: money literals in this codebase are written in
//! major_minor grouping (e.g. `1_500_00` = 1500.00) on purpose —
//! it mirrors how amounts are spoken. That trips clippy's
//! inconsistent_digit_grouping, which expects 3-digit groups;
//! the convention is deliberate, so the lint is allowed crate-wide.
#![allow(clippy::inconsistent_digit_grouping)]

//! MiraclePay SMS verification engine — parse payment SMS, match to
//! pending transactions, verify. Original clean-room implementation.
pub mod bd_templates;
pub mod matcher;
pub mod parser;
pub mod patterns_bd;
pub mod patterns_in_pk;
pub mod types;
pub mod verifier;

pub use types::{IncomingSms, ParsedSms};
pub use verifier::{parse_incoming, verify_sms, VerificationOutcome};
