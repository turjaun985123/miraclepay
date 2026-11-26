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
