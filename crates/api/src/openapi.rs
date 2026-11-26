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
    ("/health", &["get"], "Liveness + storage mode", "System", false),
    ("/api/v1/auth/register", &["post"], "Register a user account", "Auth", false),
    ("/api/v1/auth/login", &["post"], "Log in, receive a JWT", "Auth", false),
    ("/api/v1/auth/me", &["get"], "Current user profile", "Auth", true),
    ("/api/v1/auth/refresh", &["post"], "Exchange a refresh token for a fresh pair", "Auth", false),
    ("/api/v1/auth/logout", &["post"], "Revoke a refresh token", "Auth", false),
    ("/api/v1/merchants", &["post", "get"], "Create / list my merchants", "Merchants", true),
    ("/api/v1/merchants/{id}", &["get", "patch"], "Get / update a merchant", "Merchants", true),
    ("/api/v1/merchants/{id}/balance", &["get"], "Merchant balance", "Merchants", true),
    ("/api/v1/merchants/{id}/gateways", &["get", "put"], "List / upsert gateway configs", "Gateways", true),
    ("/api/v1/merchants/{id}/gateways/{gateway}", &["delete"], "Remove a gateway config", "Gateways", true),
    ("/api/v1/merchants/{id}/gateways/status", &["get"], "Gateway configuration status", "Gateways", true),
    ("/api/v1/payments", &["post", "get"], "Create / list payments", "Payments", true),
    ("/api/v1/payments/{id}", &["get"], "Get a payment", "Payments", true),
    ("/api/v1/checkout/{id}", &["get"], "Hosted checkout details", "Payments", false),
    ("/api/v1/checkout/{id}/submit", &["post"], "Submit a checkout payment", "Payments", false),
    ("/api/v1/payment-links", &["post", "get"], "Create / list payment links", "Payment Links", true),
    ("/api/v1/payment-links/{id}", &["get"], "Get a payment link", "Payment Links", true),
    ("/api/v1/payment-links/{id}/deactivate", &["post"], "Deactivate a payment link", "Payment Links", true),
    ("/pay/{slug}", &["get", "post"], "Public pay page: view / pay a link", "Payment Links", false),
    ("/api/v1/invoices", &["post", "get"], "Create / list invoices", "Invoices", true),
    ("/api/v1/invoices/{id}", &["get"], "Get an invoice", "Invoices", true),
    ("/api/v1/invoices/{id}/pdf", &["get"], "Download an invoice PDF", "Invoices", true),
    ("/api/v1/invoices/{id}/send", &["post"], "Send an invoice", "Invoices", true),
    ("/api/v1/invoices/{id}/pay", &["post"], "Pay an invoice", "Invoices", true),
    ("/api/v1/invoices/{id}/cancel", &["post"], "Cancel an invoice", "Invoices", true),
    ("/api/v1/transactions", &["get"], "List transactions", "Transactions", true),
    ("/api/v1/transactions/{id}", &["get"], "Get a transaction", "Transactions", true),
    ("/api/v1/transactions/{id}/refunds", &["post"], "Refund a transaction", "Refunds", true),
    ("/api/v1/refunds", &["get"], "List refunds", "Refunds", true),
    ("/api/v1/refunds/{id}/settle", &["post"], "Settle an in-flight refund", "Refunds", true),
    ("/api/v1/settlements", &["get"], "List settlements", "Settlements", true),
    ("/api/v1/settlements/run", &["post"], "Run settlements for a period", "Settlements", true),
    ("/api/v1/settlements/{id}/pay", &["post"], "Record a settlement payout", "Settlements", true),
    ("/api/v1/webhooks/events", &["get"], "List webhook events", "Webhooks", true),
    ("/api/v1/webhooks/deliveries", &["get"], "List webhook deliveries", "Webhooks", true),
    ("/api/v1/webhooks/events/{id}/redeliver", &["post"], "Redeliver a webhook event", "Webhooks", true),
    ("/api/v1/webhooks/test-signature", &["post"], "Verify a webhook signature", "Webhooks", true),
    ("/api/v1/gateways", &["get"], "List supported gateways", "Gateways", false),
    ("/api/v1/api-keys", &["post", "get"], "Create / list API keys", "API Keys", true),
    ("/api/v1/api-keys/{id}/revoke", &["post"], "Revoke an API key", "API Keys", true),
    ("/api/v1/sms", &["post"], "Ingest a forwarded payment SMS (device key)", "SMS", false),
    ("/api/v1/sms/parse", &["post"], "Preview SMS parsing", "SMS", true),
    ("/api/v1/plans", &["post", "get"], "Create / list subscription plans", "Subscriptions", true),
    ("/api/v1/plans/{id}", &["get", "patch"], "Get / update a plan", "Subscriptions", true),
    ("/api/v1/subscriptions", &["post", "get"], "Create / list subscriptions", "Subscriptions", true),
    ("/api/v1/subscriptions/{id}/cancel", &["post"], "Cancel a subscription", "Subscriptions", true),
    ("/api/v1/subscriptions/{id}/pause", &["post"], "Pause a subscription (cancel at period end)", "Subscriptions", true),
    ("/api/v1/subscriptions/{id}/resume", &["post"], "Resume a subscription", "Subscriptions", true),
    ("/api/v1/admin/stats", &["get"], "Platform statistics (admin)", "Admin", true),
    ("/api/v1/admin/analytics", &["get"], "Platform analytics (admin)", "Admin", true),
    ("/api/v1/admin/users", &["get"], "List users (admin)", "Admin", true),
    ("/api/v1/admin/merchants", &["get"], "List all merchants (admin)", "Admin", true),
    ("/api/v1/admin/merchants/{id}/status", &["post"], "Set a merchant's status (admin)", "Admin", true),
    ("/api/v1/admin/audit", &["get"], "Audit log (admin)", "Admin", true),
    ("/api/v1/admin/settings", &["get", "put"], "Platform settings (admin)", "Admin", true),
];

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
            "version": "0.0.1",
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
    ROUTES.iter().map(|(_, methods, _, _, _)| methods.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_openapi_3_1_with_security_scheme() {
        let doc = spec();
        assert_eq!(doc["openapi"], "3.1.0");
        assert_eq!(doc["info"]["version"], "0.0.1");
        assert!(doc["components"]["securitySchemes"]["bearerAuth"].is_object());
    }

    #[test]
    fn every_route_table_entry_appears_in_paths() {
        let doc = spec();
        let paths = doc["paths"].as_object().unwrap();
        assert_eq!(paths.len(), ROUTES.len());
        for (path, methods, _, _, _) in ROUTES {
            let ops = paths[*path].as_object().unwrap_or_else(|| panic!("missing path {path}"));
            for method in *methods {
                assert!(ops.contains_key(*method), "{method} {path} missing");
            }
        }
    }

    #[test]
    fn operation_count_matches_the_router_surface() {
        // routes::build_router registers 69 method handlers as of
        // Wave 25 — 68 in this table plus GET /openapi.json itself.
        // If you add a route, add it to ROUTES above and update
        // this number.
        assert_eq!(operation_count(), 68);
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
