//! MiraclePay API — Axum HTTP layer. Thin handlers over the domain,
//! gateways and sms-verify crates; one operation = one endpoint.
pub mod auth_middleware;
pub mod db;
pub mod dto;
pub mod handlers;
pub mod openapi;
pub mod pagination;
pub mod rate_limit;
pub mod routes;
pub mod state;
pub mod validation;

pub use routes::build_router;
pub use state::AppState;
