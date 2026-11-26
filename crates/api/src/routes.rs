//! HTTP route table for MiraclePay v0.0.1.
//!
//! Clean-room implementation. This module only wires paths to handler
//! functions; all request handling lives in `crate::handlers`, all
//! request/response shapes live in `crate::dto`, and the shared
//! application state lives in `crate::state`.
//!
//! Routing notes:
//! * Axum 0.7 path syntax is used throughout: `:id`, `:slug` and
//!   `:gateway` are path parameters (the `{id}` form is Axum 0.8+).
//! * `/api/v1/merchants/:id/gateways` and
//!   `/api/v1/merchants/:id/balance` share the `/merchants/:id` prefix
//!   but differ in their final segment, so they coexist in Axum 0.7.
//! * CORS: there is deliberately no CORS layer here. This crate does
//!   not depend on `tower_http`, and permissive CORS is a deployment
//!   concern for the fronting proxy / frontend host. If in-process
//!   CORS is ever required, add it in the binary entry point as an
//!   `axum::middleware` / `tower` layer around the router returned by
//!   [`build_router`] rather than scattering headers across handlers.

use axum::{
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Json, Router,
};

use crate::handlers::{
    admin, api_keys, auth, gateways, invoices, merchants, payment_links, payments, sms, subscriptions,
    transactions, webhooks,
};
use crate::state::AppState;

/// Liveness probe: `GET /health`.
///
/// Inline by design — it has no business logic, it simply serialises
/// the value produced by `crate::dto::health()`.
async fn health() -> impl IntoResponse {
    Json(crate::dto::health())
}

/// Build the complete MiraclePay API router.
///
/// Wires 51 method handlers across 42 distinct paths and binds the
/// shared [`AppState`] with `.with_state(state)`.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // ---------------------------------------------------------
        // Health
        // ---------------------------------------------------------
        .route("/health", get(health))
        // ---------------------------------------------------------
        // Auth
        // ---------------------------------------------------------
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/me", get(auth::me))
        // ---------------------------------------------------------
        // Merchants
        // ---------------------------------------------------------
        .route(
            "/api/v1/merchants",
            post(merchants::create_merchant).get(merchants::list_my_merchants),
        )
        .route(
            "/api/v1/merchants/:id",
            get(merchants::get_merchant).patch(merchants::update_merchant),
        )
        .route(
            "/api/v1/merchants/:id/balance",
            get(merchants::merchant_balance),
        )
        // ---------------------------------------------------------
        // Payments & checkout
        // ---------------------------------------------------------
        .route(
            "/api/v1/payments",
            post(payments::create_payment).get(payments::list_payments),
        )
        .route("/api/v1/payments/:id", get(payments::get_payment))
        .route("/api/v1/checkout/:id", get(payments::checkout_details))
        .route(
            "/api/v1/checkout/:id/submit",
            post(payments::submit_checkout),
        )
        // ---------------------------------------------------------
        // Payment links (authenticated API + public pay page)
        // ---------------------------------------------------------
        .route(
            "/api/v1/payment-links",
            post(payment_links::create_link).get(payment_links::list_links),
        )
        .route("/api/v1/payment-links/:id", get(payment_links::get_link))
        .route(
            "/api/v1/payment-links/:id/deactivate",
            post(payment_links::deactivate_link),
        )
        .route(
            "/pay/:slug",
            get(payment_links::public_link).post(payment_links::pay_link),
        )
        // ---------------------------------------------------------
        // Invoices
        // ---------------------------------------------------------
        .route(
            "/api/v1/invoices",
            post(invoices::create_invoice).get(invoices::list_invoices),
        )
        .route("/api/v1/invoices/:id", get(invoices::get_invoice))
        .route("/api/v1/invoices/:id/send", post(invoices::send_invoice))
        .route("/api/v1/invoices/:id/pay", post(invoices::pay_invoice))
        .route(
            "/api/v1/invoices/:id/cancel",
            post(invoices::cancel_invoice),
        )
        // ---------------------------------------------------------
        // Transactions & refunds
        // ---------------------------------------------------------
        .route(
            "/api/v1/transactions",
            get(transactions::list_transactions),
        )
        .route(
            "/api/v1/transactions/:id",
            get(transactions::get_transaction),
        )
        .route(
            "/api/v1/transactions/:id/refunds",
            post(transactions::create_refund),
        )
        .route("/api/v1/refunds", get(transactions::list_refunds))
        // ---------------------------------------------------------
        // Webhooks
        // ---------------------------------------------------------
        .route("/api/v1/webhooks/events", get(webhooks::list_events))
        .route(
            "/api/v1/webhooks/deliveries",
            get(webhooks::list_deliveries),
        )
        .route(
            "/api/v1/webhooks/events/:id/redeliver",
            post(webhooks::redeliver),
        )
        .route(
            "/api/v1/webhooks/test-signature",
            post(webhooks::test_signature),
        )
        // ---------------------------------------------------------
        // Gateways
        // ---------------------------------------------------------
        .route("/api/v1/gateways", get(gateways::list_gateways))
        .route(
            "/api/v1/merchants/:id/gateways",
            get(gateways::list_merchant_gateways).put(gateways::upsert_gateway_config),
        )
        .route(
            "/api/v1/merchants/:id/gateways/:gateway",
            delete(gateways::delete_gateway_config),
        )
        .route(
            "/api/v1/merchants/:id/gateways/status",
            get(gateways::gateway_status),
        )
        // ---------------------------------------------------------
        // API keys
        // ---------------------------------------------------------
        .route(
            "/api/v1/api-keys",
            post(api_keys::create_api_key).get(api_keys::list_api_keys),
        )
        .route(
            "/api/v1/api-keys/:id/revoke",
            post(api_keys::revoke_api_key),
        )
        // ---------------------------------------------------------
        // SMS verification intake
        // ---------------------------------------------------------
        .route("/api/v1/sms", post(sms::receive_sms))
        .route("/api/v1/sms/parse", post(sms::parse_preview))
        // ---------------------------------------------------------
        // Plans & subscriptions
        // ---------------------------------------------------------
        .route(
            "/api/v1/plans",
            post(subscriptions::create_plan).get(subscriptions::list_plans),
        )
        .route(
            "/api/v1/plans/:id",
            get(subscriptions::get_plan).patch(subscriptions::update_plan),
        )
        .route(
            "/api/v1/subscriptions",
            post(subscriptions::create_subscription).get(subscriptions::list_subscriptions),
        )
        .route(
            "/api/v1/subscriptions/:id/cancel",
            post(subscriptions::cancel_subscription),
        )
        .route(
            "/api/v1/subscriptions/:id/pause",
            post(subscriptions::pause_subscription),
        )
        .route(
            "/api/v1/subscriptions/:id/resume",
            post(subscriptions::resume_subscription),
        )
        // ---------------------------------------------------------
        // Admin
        // ---------------------------------------------------------
        .route("/api/v1/admin/stats", get(admin::platform_stats))
        .route("/api/v1/admin/users", get(admin::list_users))
        .route("/api/v1/admin/merchants", get(admin::list_all_merchants))
        .route(
            "/api/v1/admin/merchants/:id/status",
            post(admin::set_merchant_status),
        )
        .route("/api/v1/admin/audit", get(admin::list_audit))
        .route(
            "/api/v1/admin/settings",
            get(admin::get_settings).put(admin::update_settings),
        )
        // Shared state for every handler above.
        .with_state(state)
}
