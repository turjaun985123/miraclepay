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
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};

use crate::handlers::{
    admin, api_keys, auth, auth_flows, disputes, fx, gateways, invoices, kyc, merchants,
    payment_links, payments, plugins_admin, product, provider_webhooks, settlements, sms,
    subscriptions, transactions, webhooks,
};
use crate::state::AppState;

/// Liveness probe: `GET /health`.
///
/// Inline by design — it has no business logic, it simply serialises
/// the value produced by `crate::dto::health()`.
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let base = crate::dto::health();
    // Wave 75: persisted-store stats when Postgres is live, so an
    // operator can see the record store is actually filling.
    let (persisted_collections, persisted_records) = match &state.db_pool {
        Some(pool) => match crate::persist::persisted_stats(pool).await {
            Ok(stats) => (
                serde_json::json!(stats.len()),
                serde_json::json!(stats.iter().map(|(_, n)| n).sum::<i64>()),
            ),
            Err(_) => (serde_json::Value::Null, serde_json::Value::Null),
        },
        None => (serde_json::Value::Null, serde_json::Value::Null),
    };
    Json(serde_json::json!({
        "status": base.status,
        "version": base.version,
        "name": base.name,
        "db": state.db_mode().as_str(),
        // Wave 46: sandbox deployments are discoverable, never stealthy.
        "sandbox": crate::seed::sandbox_enabled(),
        "persisted_collections": persisted_collections,
        "persisted_records": persisted_records,
    }))
}

/// Liveness probe: `GET /health/live` — the process is up.
async fn health_live() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "version": "0.0.3.2"}))
}

/// Readiness probe: `GET /health/ready` — dependencies are
/// reachable. In Postgres mode this pings the database; a failed
/// ping answers 503 so orchestrators stop routing traffic.
async fn health_ready(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = match &state.db_pool {
        Some(pool) => sqlx::query("SELECT 1").execute(pool).await.is_ok(),
        None => true,
    };
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(serde_json::json!({
            "status": if db_ok { "ready" } else { "not_ready" },
            "db": state.db_mode().as_str(),
            "db_reachable": db_ok,
        })),
    )
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
        // Wave 94: liveness (process up) vs readiness (deps up).
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
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
        .route(
            "/api/v1/auth/password-reset/request",
            post(auth_flows::password_reset_request),
        )
        .route(
            "/api/v1/auth/password-reset/confirm",
            post(auth_flows::password_reset_confirm),
        )
        .route(
            "/api/v1/auth/email-verification/request",
            post(auth_flows::email_verification_request),
        )
        .route(
            "/api/v1/auth/email-verification/confirm",
            post(auth_flows::email_verification_confirm),
        )
        .route("/api/v1/auth/sessions", get(auth_flows::list_sessions))
        .route(
            "/api/v1/auth/sessions/:id",
            delete(auth_flows::revoke_session),
        )
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
        // Public (unauthenticated) payment-link API — the canonical
        // contract the hosted pay page calls. It lives under
        // /api/v1 like every other API route; the browser page path
        // /pay/:slug is NOT an API route (the embedded frontend
        // serves that shell from the static fallback). Until
        // v0.0.3.2 these handlers were mounted on /pay/:slug, which
        // both broke the frontend's /api/v1 calls (404) and shadowed
        // the pay page's own URL with a JSON response.
        .route(
            "/api/v1/payment-links/public/:slug",
            get(payment_links::public_link),
        )
        .route(
            "/api/v1/payment-links/public/:slug/pay",
            post(payment_links::pay_link),
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
        .route("/api/v1/transactions", get(transactions::list_transactions))
        .route(
            "/api/v1/transactions/:id",
            get(transactions::get_transaction),
        )
        .route(
            "/api/v1/transactions/:id/refunds",
            post(transactions::create_refund),
        )
        .route(
            "/api/v1/transactions/:id/verify",
            post(plugins_admin::verify_transaction),
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
        .route(
            "/api/v1/disputes",
            post(disputes::open_dispute).get(disputes::list_disputes),
        )
        .route("/api/v1/disputes/:id", get(disputes::get_dispute))
        .route(
            "/api/v1/disputes/:id/evidence",
            post(disputes::add_evidence),
        )
        .route(
            "/api/v1/disputes/:id/evidence/files",
            post(disputes::add_evidence_file),
        )
        // Product depth (Waves 96-113)
        .route("/api/v1/merchants/:id/onboarding", get(product::onboarding))
        .route(
            "/api/v1/merchants/:id/team",
            get(product::list_team).post(product::invite_team_member),
        )
        .route(
            "/api/v1/merchants/:id/team/:member_id",
            delete(product::remove_team_member),
        )
        .route(
            "/api/v1/transactions/export.csv",
            get(product::export_transactions_csv),
        )
        .route("/api/v1/search", get(product::search))
        .route(
            "/api/v1/gateways/:provider/test",
            post(product::test_gateway),
        )
        .route("/api/v1/payment-links/:id/stats", get(product::link_stats))
        .route(
            "/api/v1/transactions/:id/receipt.pdf",
            get(product::receipt_pdf),
        )
        .route(
            "/api/v1/admin/users/:id/suspend",
            post(product::admin_suspend_user),
        )
        .route(
            "/api/v1/admin/users/:id/activate",
            post(product::admin_activate_user),
        )
        .route(
            "/api/v1/admin/disputes/:id/decide",
            post(disputes::decide_dispute),
        )
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
        // Provider receivers (Waves 80-82): Stripe/PayPal webhooks
        // and the bKash tokenized callback. Public by necessity —
        // each verifies the provider's own proof before settling.
        .route(
            "/api/v1/webhooks/stripe",
            post(provider_webhooks::stripe_webhook),
        )
        .route(
            "/api/v1/webhooks/paypal",
            post(provider_webhooks::paypal_webhook),
        )
        .route(
            "/api/v1/callbacks/bkash",
            post(provider_webhooks::bkash_callback),
        )
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
        .route("/api/v1/admin/plugins", get(plugins_admin::list_plugins))
        .route(
            "/api/v1/admin/plugins/:id/enable",
            post(plugins_admin::enable_plugin),
        )
        .route(
            "/api/v1/admin/plugins/:id/disable",
            post(plugins_admin::disable_plugin),
        )
        .route(
            "/api/v1/admin/plugins/:id/config",
            get(plugins_admin::get_plugin_config).put(plugins_admin::put_plugin_config),
        )
        .route(
            "/api/v1/admin/plugins/:id/invoke",
            post(plugins_admin::invoke_plugin),
        )
        .route(
            "/api/v1/admin/plugins/:id/logs",
            get(plugins_admin::plugin_logs),
        )
        .route("/api/v1/admin/audit", get(admin::list_audit))
        .route(
            "/api/v1/admin/settings",
            get(admin::get_settings).put(admin::update_settings),
        )
        // Wave 40: structured fallback for unmatched paths
        // (unsupported API versions get a named error).
        // v0.0.3.2: the fallback now serves the embedded frontend
        // for browser paths first (see `static_files`); API paths
        // still land in the Wave 40 structured JSON 404.
        .fallback(crate::static_files::static_fallback)
        // Wave 40: API-Version response header on /api/v1 calls.
        .layer(axum::middleware::from_fn(
            crate::versioning::version_middleware,
        ))
        // Wave 44: structured JSON request log + x-request-id.
        .layer(axum::middleware::from_fn(
            crate::metrics::request_log_middleware,
        ))
        // Wave 12: rate-limit middleware (auth 10/min, SMS ingest
        // 120/min, general API 600/min per client; see `rate_limit`).
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::rate_limit::rate_limit_middleware,
        ))
        // Wave 24: security headers + CORS — outermost layer, so
        // headers are present even on rate-limit and error responses.
        .layer(axum::middleware::from_fn(
            crate::security::security_middleware,
        ))
        // Shared state for every handler above.
        .with_state(state)
}
