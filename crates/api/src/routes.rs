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
    extract::State,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Json, Router,
};

use crate::handlers::{
    admin, api_keys, auth, disputes, fx, gateways, invoices, kyc, merchants, payment_links,
    payments, settlements, sms, subscriptions, transactions, webhooks,
};
use crate::state::AppState;

/// Liveness probe: `GET /health`.
///
/// Inline by design — it has no business logic, it simply serialises
/// the value produced by `crate::dto::health()`.
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let base = crate::dto::health();
    Json(serde_json::json!({
        "status": base.status,
        "version": base.version,
        "name": base.name,
        "db": state.db_mode().as_str(),
        // Wave 46: sandbox deployments are discoverable, never stealthy.
        "sandbox": crate::seed::sandbox_enabled(),
    }))
}

/// OpenAPI document: `GET /openapi.json` (Wave 20; served from a
/// once-built cache since Wave 28).
async fn openapi_spec() -> impl IntoResponse {
    Json(crate::openapi::spec_cached().clone())
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
        .route("/metrics", get(crate::metrics::metrics_handler))
        .route("/openapi.json", get(openapi_spec))
        // ---------------------------------------------------------
        // Auth
        // ---------------------------------------------------------
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/me", get(auth::me))
        .route("/api/v1/auth/refresh", post(auth::refresh))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/auth/2fa/setup", post(auth::totp_setup))
        .route("/api/v1/auth/2fa/enable", post(auth::totp_enable))
        .route("/api/v1/auth/2fa/disable", post(auth::totp_disable))
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
        .route(
            "/api/v1/merchants/:id/settings",
            get(merchants::get_merchant_settings).put(merchants::update_merchant_settings),
        )
        .route(
            "/api/v1/merchants/:id/analytics",
            get(merchants::merchant_analytics),
        )
        // Merchant KYC-lite (Wave 33)
        .route(
            "/api/v1/merchants/:id/kyc",
            get(kyc::get_kyc).put(kyc::submit_kyc),
        )
        .route("/api/v1/admin/kyc/pending", get(kyc::list_pending_kyc))
        .route(
            "/api/v1/admin/merchants/:id/kyc/decision",
            post(kyc::decide_kyc),
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
        .route("/api/v1/invoices/:id/pdf", get(invoices::get_invoice_pdf))
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
        .route(
            "/api/v1/refunds/:id/settle",
            post(transactions::settle_refund),
        )
        // ---------------------------------------------------------
        // Settlements (Wave 18)
        // ---------------------------------------------------------
        .route("/api/v1/settlements", get(settlements::list_settlements))
        .route(
            "/api/v1/settlements/run",
            post(settlements::run_settlements),
        )
        .route(
            "/api/v1/settlements/:id/pay",
            post(settlements::pay_settlement),
        )
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
        // Disputes / chargebacks (Wave 38)
        .route("/api/v1/disputes", post(disputes::open_dispute).get(disputes::list_disputes))
        .route("/api/v1/disputes/:id", get(disputes::get_dispute))
        .route("/api/v1/disputes/:id/evidence", post(disputes::add_evidence))
        .route("/api/v1/admin/disputes/:id/decide", post(disputes::decide_dispute))
        // FX conversion (Wave 34)
        .route("/api/v1/fx/rates", get(fx::list_rates))
        .route("/api/v1/fx/convert", get(fx::convert))
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
        // API version discovery (Wave 40, public).
        .route("/api/versions", get(crate::versioning::versions))
        // Admin
        // ---------------------------------------------------------
        .route("/api/v1/admin/stats", get(admin::platform_stats))
        .route("/api/v1/admin/analytics", get(admin::platform_analytics))
        .route(
            "/api/v1/admin/notifications/dispatch",
            post(admin::dispatch_notifications),
        )
        .route("/api/v1/admin/fraud/flagged", get(admin::fraud_flagged))
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
        // Wave 40: structured fallback for unmatched paths
        // (unsupported API versions get a named error).
        .fallback(crate::versioning::api_fallback)
        // Wave 40: API-Version response header on /api/v1 calls.
        .layer(axum::middleware::from_fn(crate::versioning::version_middleware))
        // Wave 44: structured JSON request log + x-request-id.
        .layer(axum::middleware::from_fn(crate::metrics::request_log_middleware))
        // Wave 12: rate-limit middleware (auth 10/min, SMS ingest
        // 120/min, general API 600/min per client; see `rate_limit`).
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::rate_limit::rate_limit_middleware,
        ))
        // Wave 24: security headers + CORS — outermost layer, so
        // headers are present even on rate-limit and error responses.
        .layer(axum::middleware::from_fn(crate::security::security_middleware))
        // Shared state for every handler above.
        .with_state(state)
}
