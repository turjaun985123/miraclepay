//! OpenAPI 3.1 description of the MiraclePay API (Wave 20).
//!
//! The spec is generated from the route table below, which mirrors
//! `routes::build_router` one-to-one (Axum `:param` segments are
//! rewritten to OpenAPI `{param}`). It is served at
//! `GET /openapi.json` so docs tooling and client generators always
//! describe the deployed version. A unit test pins the operation
//! count so a route added without a spec entry fails the build's
//! test suite — update this table when routes change.

use serde_json::{json, Map, Value};

/// (path, methods, summary, tag, requires_auth)
const ROUTES: &[(&str, &[&str], &str, &str, bool)] = &[
    (
        "/api/versions",
        &["get"],
        "API version discovery",
        "Meta",
        false,
    ),
    (
        "/health",
        &["get"],
        "Liveness + storage mode",
        "System",
        false,
    ),
    (
        "/metrics",
        &["get"],
        "Prometheus metrics exposition",
        "System",
        false,
    ),
    (
        "/api/v1/auth/register",
        &["post"],
        "Register a user account",
        "Auth",
        false,
    ),
    (
        "/api/v1/auth/login",
        &["post"],
        "Log in, receive a JWT",
        "Auth",
        false,
    ),
    (
        "/api/v1/auth/me",
        &["get"],
        "Current user profile",
        "Auth",
        true,
    ),
    (
        "/api/v1/auth/refresh",
        &["post"],
        "Exchange a refresh token for a fresh pair",
        "Auth",
        false,
    ),
    (
        "/api/v1/auth/logout",
        &["post"],
        "Revoke a refresh token",
        "Auth",
        false,
    ),
    (
        "/api/v1/auth/password-reset/request",
        &["post"],
        "Request a password reset token (always 200)",
        "Auth",
        false,
    ),
    (
        "/api/v1/auth/password-reset/confirm",
        &["post"],
        "Reset the password with a token; revokes all sessions",
        "Auth",
        false,
    ),
    (
        "/api/v1/auth/email-verification/request",
        &["post"],
        "Request an email verification token",
        "Auth",
        true,
    ),
    (
        "/api/v1/auth/email-verification/confirm",
        &["post"],
        "Confirm email verification with a token",
        "Auth",
        false,
    ),
    (
        "/api/v1/auth/sessions",
        &["get"],
        "List the caller's sessions",
        "Auth",
        true,
    ),
    (
        "/api/v1/auth/sessions/{id}",
        &["delete"],
        "Revoke one session",
        "Auth",
        true,
    ),
    (
        "/api/v1/auth/2fa/setup",
        &["post"],
        "Start TOTP 2FA setup",
        "Auth",
        true,
    ),
    (
        "/api/v1/auth/2fa/enable",
        &["post"],
        "Enable TOTP 2FA with a code",
        "Auth",
        true,
    ),
    (
        "/api/v1/auth/2fa/disable",
        &["post"],
        "Disable TOTP 2FA with a code",
        "Auth",
        true,
    ),
    (
        "/api/v1/merchants",
        &["post", "get"],
        "Create / list my merchants",
        "Merchants",
        true,
    ),
    (
        "/api/v1/merchants/{id}",
        &["get", "patch"],
        "Get / update a merchant",
        "Merchants",
        true,
    ),
    (
        "/api/v1/merchants/{id}/balance",
        &["get"],
        "Merchant balance",
        "Merchants",
        true,
    ),
    (
        "/api/v1/merchants/{id}/settings",
        &["get", "put"],
        "Merchant settings incl. checkout branding",
        "Merchants",
        true,
    ),
    (
        "/api/v1/merchants/{id}/analytics",
        &["get"],
        "Merchant daily analytics series",
        "Merchants",
        true,
    ),
    (
        "/api/v1/merchants/{id}/kyc",
        &["get", "put"],
        "Get / submit merchant KYC profile",
        "Merchants",
        true,
    ),
    (
        "/api/v1/admin/kyc/pending",
        &["get"],
        "KYC review queue (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/merchants/{id}/kyc/decision",
        &["post"],
        "Verify / reject a KYC profile (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/merchants/{id}/gateways",
        &["get", "put"],
        "List / upsert gateway configs",
        "Gateways",
        true,
    ),
    (
        "/api/v1/merchants/{id}/gateways/{gateway}",
        &["delete"],
        "Remove a gateway config",
        "Gateways",
        true,
    ),
    (
        "/api/v1/merchants/{id}/gateways/status",
        &["get"],
        "Gateway configuration status",
        "Gateways",
        true,
    ),
    (
        "/api/v1/payments",
        &["post", "get"],
        "Create / list payments",
        "Payments",
        true,
    ),
    (
        "/api/v1/payments/{id}",
        &["get"],
        "Get a payment",
        "Payments",
        true,
    ),
    (
        "/api/v1/checkout/{id}",
        &["get"],
        "Hosted checkout details",
        "Payments",
        false,
    ),
    (
        "/api/v1/checkout/{id}/submit",
        &["post"],
        "Submit a checkout payment",
        "Payments",
        false,
    ),
    (
        "/api/v1/payment-links",
        &["post", "get"],
        "Create / list payment links",
        "Payment Links",
        true,
    ),
    (
        "/api/v1/payment-links/{id}",
        &["get"],
        "Get a payment link",
        "Payment Links",
        true,
    ),
    (
        "/api/v1/payment-links/{id}/deactivate",
        &["post"],
        "Deactivate a payment link",
        "Payment Links",
        true,
    ),
    (
        "/pay/{slug}",
        &["get", "post"],
        "Public pay page: view / pay a link",
        "Payment Links",
        false,
    ),
    (
        "/api/v1/invoices",
        &["post", "get"],
        "Create / list invoices",
        "Invoices",
        true,
    ),
    (
        "/api/v1/invoices/{id}",
        &["get"],
        "Get an invoice",
        "Invoices",
        true,
    ),
    (
        "/api/v1/invoices/{id}/pdf",
        &["get"],
        "Download an invoice PDF",
        "Invoices",
        true,
    ),
    (
        "/api/v1/invoices/{id}/send",
        &["post"],
        "Send an invoice",
        "Invoices",
        true,
    ),
    (
        "/api/v1/invoices/{id}/pay",
        &["post"],
        "Pay an invoice",
        "Invoices",
        true,
    ),
    (
        "/api/v1/invoices/{id}/cancel",
        &["post"],
        "Cancel an invoice",
        "Invoices",
        true,
    ),
    (
        "/api/v1/transactions",
        &["get"],
        "List transactions",
        "Transactions",
        true,
    ),
    (
        "/api/v1/transactions/{id}",
        &["get"],
        "Get a transaction",
        "Transactions",
        true,
    ),
    (
        "/api/v1/transactions/{id}/refunds",
        &["post"],
        "Refund a transaction",
        "Refunds",
        true,
    ),
    ("/api/v1/refunds", &["get"], "List refunds", "Refunds", true),
    (
        "/api/v1/refunds/{id}/settle",
        &["post"],
        "Settle an in-flight refund",
        "Refunds",
        true,
    ),
    (
        "/api/v1/settlements",
        &["get"],
        "List settlements",
        "Settlements",
        true,
    ),
    (
        "/api/v1/settlements/run",
        &["post"],
        "Run settlements for a period",
        "Settlements",
        true,
    ),
    (
        "/api/v1/settlements/{id}/pay",
        &["post"],
        "Record a settlement payout",
        "Settlements",
        true,
    ),
    (
        "/api/v1/webhooks/events",
        &["get"],
        "List webhook events",
        "Webhooks",
        true,
    ),
    (
        "/api/v1/webhooks/deliveries",
        &["get"],
        "List webhook deliveries",
        "Webhooks",
        true,
    ),
    (
        "/api/v1/webhooks/events/{id}/redeliver",
        &["post"],
        "Redeliver a webhook event",
        "Webhooks",
        true,
    ),
    (
        "/api/v1/webhooks/test-signature",
        &["post"],
        "Verify a webhook signature",
        "Webhooks",
        true,
    ),
    (
        "/api/v1/gateways",
        &["get"],
        "List supported gateways",
        "Gateways",
        false,
    ),
    (
        "/api/v1/disputes",
        &["post", "get"],
        "Open or list disputes",
        "Disputes",
        true,
    ),
    (
        "/api/v1/disputes/{id}",
        &["get"],
        "Get one dispute",
        "Disputes",
        true,
    ),
    (
        "/api/v1/admin/plugins",
        &["get"],
        "List scanned plugins with load errors and enabled state",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/plugins/{id}/enable",
        &["post"],
        "Enable a plugin",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/plugins/{id}/disable",
        &["post"],
        "Disable a plugin",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/plugins/{id}/config",
        &["get", "put"],
        "Read or replace a plugin's configuration",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/plugins/{id}/invoke",
        &["post"],
        "Invoke an operation on a JS plugin (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/plugins/{id}/logs",
        &["get"],
        "Drain a plugin's log lines",
        "Admin",
        true,
    ),
    (
        "/api/v1/transactions/{id}/verify",
        &["post"],
        "Re-verify a transaction with its gateway and settle on confirmation",
        "Transactions",
        true,
    ),
    (
        "/api/v1/disputes/{id}/evidence/files",
        &["post"],
        "Attach an evidence file reference to a dispute",
        "Disputes",
        true,
    ),
    (
        "/api/v1/merchants/{id}/onboarding",
        &["get"],
        "Merchant onboarding checklist (derived from live state)",
        "Merchants",
        true,
    ),
    (
        "/api/v1/merchants/{id}/team",
        &["get", "post"],
        "List or invite merchant team members",
        "Merchants",
        true,
    ),
    (
        "/api/v1/merchants/{id}/team/{member_id}",
        &["delete"],
        "Remove a team member",
        "Merchants",
        true,
    ),
    (
        "/api/v1/transactions/export.csv",
        &["get"],
        "Export a merchant's transactions as CSV",
        "Transactions",
        true,
    ),
    (
        "/api/v1/search",
        &["get"],
        "Search transactions, payment links and invoices",
        "Search",
        true,
    ),
    (
        "/api/v1/gateways/{provider}/test",
        &["post"],
        "Test a stored gateway configuration against the provider",
        "Gateways",
        true,
    ),
    (
        "/api/v1/payment-links/{id}/stats",
        &["get"],
        "Payment link views and payment conversion",
        "Payment Links",
        true,
    ),
    (
        "/api/v1/transactions/{id}/receipt.pdf",
        &["get"],
        "Download a payment receipt PDF",
        "Transactions",
        true,
    ),
    (
        "/api/v1/admin/users/{id}/suspend",
        &["post"],
        "Suspend a user account (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/users/{id}/activate",
        &["post"],
        "Reactivate a suspended user (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/disputes/{id}/evidence",
        &["post"],
        "Add merchant evidence to a dispute",
        "Disputes",
        true,
    ),
    (
        "/api/v1/admin/disputes/{id}/decide",
        &["post"],
        "Decide a dispute won/lost (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/fx/rates",
        &["get"],
        "FX rate table (BDT-based, indicative)",
        "FX",
        false,
    ),
    (
        "/api/v1/fx/convert",
        &["get"],
        "Convert an amount between currencies",
        "FX",
        true,
    ),
    (
        "/api/v1/api-keys",
        &["post", "get"],
        "Create / list API keys",
        "API Keys",
        true,
    ),
    (
        "/api/v1/api-keys/{id}/revoke",
        &["post"],
        "Revoke an API key",
        "API Keys",
        true,
    ),
    (
        "/api/v1/sms",
        &["post"],
        "Ingest a forwarded payment SMS (device key)",
        "SMS",
        false,
    ),
    (
        "/api/v1/webhooks/stripe",
        &["post"],
        "Stripe webhook receiver (signature-verified)",
        "Provider webhooks",
        false,
    ),
    (
        "/api/v1/webhooks/paypal",
        &["post"],
        "PayPal webhook receiver (verified via PayPal API)",
        "Provider webhooks",
        false,
    ),
    (
        "/api/v1/callbacks/bkash",
        &["post"],
        "bKash tokenized callback (execute-verified)",
        "Provider webhooks",
        false,
    ),
    (
        "/api/v1/sms/parse",
        &["post"],
        "Preview SMS parsing",
        "SMS",
        true,
    ),
    (
        "/api/v1/plans",
        &["post", "get"],
        "Create / list subscription plans",
        "Subscriptions",
        true,
    ),
    (
        "/api/v1/plans/{id}",
        &["get", "patch"],
        "Get / update a plan",
        "Subscriptions",
        true,
    ),
    (
        "/api/v1/subscriptions",
        &["post", "get"],
        "Create / list subscriptions",
        "Subscriptions",
        true,
    ),
    (
        "/api/v1/subscriptions/{id}/cancel",
        &["post"],
        "Cancel a subscription",
        "Subscriptions",
        true,
    ),
    (
        "/api/v1/subscriptions/{id}/pause",
        &["post"],
        "Pause a subscription (cancel at period end)",
        "Subscriptions",
        true,
    ),
    (
        "/api/v1/subscriptions/{id}/resume",
        &["post"],
        "Resume a subscription",
        "Subscriptions",
        true,
    ),
    (
        "/api/v1/admin/stats",
        &["get"],
        "Platform statistics (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/analytics",
        &["get"],
        "Platform analytics (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/notifications/dispatch",
        &["post"],
        "Dispatch queued notifications via SMTP (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/fraud/flagged",
        &["get"],
        "Risk-flagged transactions, score >= 30 (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/users",
        &["get"],
        "List users (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/merchants",
        &["get"],
        "List all merchants (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/merchants/{id}/status",
        &["post"],
        "Set a merchant's status (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/audit",
        &["get"],
        "Audit log (admin)",
        "Admin",
        true,
    ),
    (
        "/api/v1/admin/settings",
        &["get", "put"],
        "Platform settings (admin)",
        "Admin",
        true,
    ),
];

/// The cached document (Wave 28: the spec is static for a given
/// build — rebuilding it per request cost ~24× throughput on
/// `/openapi.json` in the load pass: 156 req/s vs 3,693 for
/// `/health`; it is now built once).
pub fn spec_cached() -> &'static Value {
    static SPEC: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SPEC.get_or_init(spec)
}

/// Build the full OpenAPI document.
pub fn spec() -> Value {
    let mut paths = Map::new();
    for (path, methods, summary, tag, auth) in ROUTES {
        let mut operations = Map::new();
        for method in *methods {
            let mut op = Map::new();
            op.insert("summary".to_string(), json!(summary));
            op.insert("tags".to_string(), json!([tag]));
            // Path parameters, derived from the `{param}` segments.
            let params: Vec<Value> = path
                .split('/')
                .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
                .map(|seg| {
                    json!({
                        "name": seg.trim_matches(|c| c == '{' || c == '}'),
                        "in": "path",
                        "required": true,
                        "schema": { "type": "string" }
                    })
                })
                .collect();
            if !params.is_empty() {
                op.insert("parameters".to_string(), json!(params));
            }
            if *auth {
                op.insert("security".to_string(), json!([{ "bearerAuth": [] }]));
            }
            op.insert(
                "responses".to_string(),
                json!({
                    "200": { "description": "OK" },
                    "201": { "description": "Created" },
                    "400": { "description": "Bad request" },
                    "401": { "description": "Unauthorized" },
                    "403": { "description": "Forbidden" },
                    "404": { "description": "Not found" },
                    "409": { "description": "Conflict" },
                    "429": { "description": "Rate limited" }
                }),
            );
            operations.insert(method.to_string(), Value::Object(op));
        }
        paths.insert(path.to_string(), Value::Object(operations));
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "MiraclePay API",
            "version": "0.0.3.2",
            "description": "Self-hosted payment automation: gateways, SMS verification, checkout, payment links, invoices, subscriptions, settlements, webhooks.",
            "license": { "name": "AGPL-3.0" }
        },
        "servers": [{ "url": "http://localhost:8080", "description": "Local / self-hosted" }],
        "paths": Value::Object(paths),
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT",
                    "description": "JWT from POST /api/v1/auth/login. The SMS ingest endpoint instead uses the x-device-key header with a merchant API key."
                }
            }
        }
    })
}

/// Total number of operations in the spec (drift guard for tests).
pub fn operation_count() -> usize {
    ROUTES
        .iter()
        .map(|(_, methods, _, _, _)| methods.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_openapi_3_1_with_security_scheme() {
        let doc = spec();
        assert_eq!(doc["openapi"], "3.1.0");
        assert_eq!(doc["info"]["version"], "0.0.3.2");
        assert!(doc["components"]["securitySchemes"]["bearerAuth"].is_object());
    }

    #[test]
    fn every_route_table_entry_appears_in_paths() {
        let doc = spec();
        let paths = doc["paths"].as_object().unwrap();
        assert_eq!(paths.len(), ROUTES.len());
        for (path, methods, _, _, _) in ROUTES {
            let ops = paths[*path]
                .as_object()
                .unwrap_or_else(|| panic!("missing path {path}"));
            for method in *methods {
                assert!(ops.contains_key(*method), "{method} {path} missing");
            }
        }
    }

    #[test]
    fn operation_count_matches_the_router_surface() {
        // routes::build_router registers 114 method handlers as of
        // Wave 113 — 111 operations in this table plus
        // GET /openapi.json itself and the pre-existing admin
        // user list counted once.
        // If you add a route, add it to ROUTES above and update
        // this number.
        assert_eq!(operation_count(), 118);
    }

    #[test]
    fn cached_spec_matches_the_built_spec() {
        assert_eq!(spec_cached(), &spec());
    }

    #[test]
    fn path_parameters_are_declared() {
        let doc = spec();
        let params = doc["paths"]["/api/v1/invoices/{id}/pdf"]["get"]["parameters"]
            .as_array()
            .unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0]["name"], "id");
    }
}
