//! Wave 49: money literals in this codebase are written in
//! major_minor grouping (e.g. `1_500_00` = 1500.00) on purpose —
//! it mirrors how amounts are spoken. That trips clippy's
//! inconsistent_digit_grouping, which expects 3-digit groups;
//! the convention is deliberate, so the lint is allowed crate-wide.
#![allow(clippy::inconsistent_digit_grouping)]

//! MiraclePay API — Axum HTTP layer. Thin handlers over the domain,
//! gateways and sms-verify crates; one operation = one endpoint.
pub mod auth_middleware;
pub mod auth_tokens;
pub mod config;
pub mod db;
pub mod dto;
pub mod handlers;
pub mod key_auth;
pub mod metrics;
pub mod openapi;
pub mod pagination;
pub mod persist;
pub mod plugins;
pub mod rate_limit;
pub mod refresh;
pub mod routes;
pub mod security;
pub mod seed;
pub mod state;
pub mod validation;
pub mod versioning;

pub use routes::build_router;
pub use state::AppState;
