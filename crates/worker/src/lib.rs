//! Wave 49: money literals in this codebase are written in
//! major_minor grouping (e.g. `1_500_00` = 1500.00) on purpose —
//! it mirrors how amounts are spoken. That trips clippy's
//! inconsistent_digit_grouping, which expects 3-digit groups;
//! the convention is deliberate, so the lint is allowed crate-wide.
#![allow(clippy::inconsistent_digit_grouping)]

//! MiraclePay worker — background jobs: webhook delivery with signed
//! retries, settlement computation, notification dispatch.
pub mod delivery_store;
pub mod notification_job;
pub mod settlement_job;
pub mod smtp_transport;
pub mod subscription_job;
pub mod webhook_delivery;
