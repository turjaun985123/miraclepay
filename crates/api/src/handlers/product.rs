//! Product-depth endpoints (Waves 96–113).
//!
//! - Wave 96: `GET /merchants/:id/onboarding` — a checklist derived
//!   from live state (nothing is stored; every step is computed).
//! - Wave 102: merchant team members — list / invite / remove.
//! - Wave 105: `GET /transactions/export.csv` — CSV download.
//! - Wave 106: `GET /search` — transactions, links and invoices.
//! - Wave 108: `POST /gateways/:provider/test` — live credential
//!   check (bKash grant / PayPal OAuth / Stripe balance); without
//!   credentials the honest answer is NotConfigured.
//! - Wave 111: `GET /payment-links/:id/stats` — views + payments.
//! - Wave 112: `GET /transactions/:id/receipt.pdf`.
//! - Wave 113: admin user management — list / suspend / activate.

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use miraclepay_domain::api_key::ApiScope;

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

async fn merchant_owned(state: &AppState, auth: &AuthUser, merchant_id: &str) -> bool {
    if auth.role == "admin" || auth.merchant_id.as_deref() == Some(merchant_id) {
        return true;
    }
    state
        .merchants
        .read()
        .await
        .get(merchant_id)
        .map(|m| m.owner_user_id == auth.user_id)
        .unwrap_or(false)
}

fn require_admin(auth: &AuthUser) -> Result<(), ApiError> {
    if auth.role == "admin" {
        Ok(())
    } else {
        Err(ApiError::Forbidden("admin only".to_string()))
    }
}

// ---------------------------------------------------------------------
// Wave 96 — onboarding checklist
// ---------------------------------------------------------------------

/// `GET /merchants/:id/onboarding`
pub async fn onboarding(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !merchant_owned(&state, &auth, &merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    let merchant = state
        .merchants
        .read()
        .await
        .get(&merchant_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound("merchant".to_string()))?;
    let owner = state
        .users
        .read()
        .await
        .get(&merchant.owner_user_id)
        .cloned();
    let steps = [
        (
            "email_verified",
            "Verify your email address",
            owner.map(|u| u.email_verified).unwrap_or(false),
        ),
        (
            "gateway_configured",
            "Configure a payment gateway",
            state
                .gateway_configs
                .read()
                .await
                .get(&merchant_id)
                .map(|v| !v.is_empty())
                .unwrap_or(false),
        ),
        (
            "kyc_submitted",
            "Submit KYC documents",
            state
                .kyc_profiles
                .read()
                .await
                .values()
                .any(|k| k.merchant_id == merchant_id),
        ),
        (
            "payment_link_created",
            "Create your first payment link",
            state
                .payment_links
                .read()
                .await
                .values()
                .any(|l| l.merchant_id == merchant_id),
        ),
        (
            "api_key_created",
            "Create an API key",
            state
                .api_keys
                .read()
                .await
                .values()
                .any(|k| k.merchant_id == merchant_id),
        ),
        (
            "branding_set",
            "Set your checkout branding",
            state
                .merchant_settings
                .read()
                .await
                .get(&merchant_id)
                .map(|s| s.brand_name.is_some())
                .unwrap_or(false),
        ),
        (
            "first_payment",
            "Receive your first payment",
            state
                .transactions
                .read()
                .await
                .values()
                .any(|t| t.merchant_id == merchant_id),
        ),
    ];
    let done = steps.iter().filter(|(_, _, done)| *done).count();
    let total = steps.len();
    Ok(Json(json!({
        "merchant_id": merchant_id,
        "steps": steps.iter().map(|(id, label, done)| json!({"id": id, "label": label, "done": done})).collect::<Vec<_>>(),
        "completed": done,
        "total": total,
        "percent": (done * 100).checked_div(total).unwrap_or(100),
    })))
}

// ---------------------------------------------------------------------
// Wave 102 — team members
// ---------------------------------------------------------------------

/// A merchant team membership (Wave 102).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub id: String,
    pub merchant_id: String,
    pub email: String,
    /// "admin" | "staff" | "viewer" (merchant-scoped roles).
    pub role: String,
    pub invited_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct InviteReq {
    pub email: String,
    pub role: String,
}

/// `GET /merchants/:id/team`
pub async fn list_team(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !merchant_owned(&state, &auth, &merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    let members: Vec<Value> = state
        .merchant_members
        .read()
        .await
        .values()
        .filter(|m| m.merchant_id == merchant_id)
        .map(|m| json!({"id": m.id, "email": m.email, "role": m.role, "created_at": m.created_at}))
        .collect();
    Ok(Json(json!({"members": members})))
}

/// `POST /merchants/:id/team` — invite by email + role.
pub async fn invite_team_member(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(merchant_id): Path<String>,
    Json(req): Json<InviteReq>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !merchant_owned(&state, &auth, &merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    if !matches!(req.role.as_str(), "admin" | "staff" | "viewer") {
        return Err(ApiError::BadRequest(
            "role must be admin, staff or viewer".to_string(),
        ));
    }
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::BadRequest("invalid email".to_string()));
    }
    let member = TeamMember {
        id: miraclepay_domain::ids::new_id("tmb"),
        merchant_id: merchant_id.clone(),
        email: email.clone(),
        role: req.role.clone(),
        invited_by: auth.user_id.clone(),
        created_at: chrono::Utc::now(),
    };
    let mut members = state.merchant_members.write().await;
    if members
        .values()
        .any(|m| m.merchant_id == merchant_id && m.email == email)
    {
        return Err(ApiError::Conflict("already a team member".to_string()));
    }
    members.insert(member.id.clone(), member.clone());
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": member.id, "email": member.email, "role": member.role})),
    ))
}

/// `DELETE /merchants/:id/team/:member_id`
pub async fn remove_team_member(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((merchant_id, member_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if !merchant_owned(&state, &auth, &merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    let mut members = state.merchant_members.write().await;
    match members.get(&member_id) {
        Some(m) if m.merchant_id == merchant_id => {
            members.remove(&member_id);
            Ok(Json(json!({"ok": true})))
        }
        _ => Err(ApiError::NotFound("team member".to_string())),
    }
}

// ---------------------------------------------------------------------
// Wave 105 — CSV export
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub merchant_id: String,
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// `GET /transactions/export.csv?merchant_id=`
pub async fn export_transactions_csv(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !merchant_owned(&state, &auth, &query.merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    let mut rows = vec!["id,created_at,gateway,amount_minor,currency,status,reference".to_string()];
    let mut txs: Vec<_> = state
        .transactions
        .read()
        .await
        .values()
        .filter(|t| t.merchant_id == query.merchant_id)
        .cloned()
        .collect();
    txs.sort_by_key(|t| t.created_at);
    for tx in txs {
        rows.push(
            [
                tx.id.clone(),
                tx.created_at.to_rfc3339(),
                tx.gateway.clone(),
                tx.amount.amount_minor.to_string(),
                tx.amount.currency.code().to_string(),
                format!("{:?}", tx.status),
                tx.reference.clone().unwrap_or_default(),
            ]
            .iter()
            .map(|f| csv_escape(f))
            .collect::<Vec<_>>()
            .join(","),
        );
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"transactions.csv\"",
            ),
        ],
        rows.join("\n"),
    ))
}

// ---------------------------------------------------------------------
// Wave 106 — dashboard search
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub merchant_id: String,
}

/// `GET /search?q=&merchant_id=`
pub async fn search(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, ApiError> {
    if !merchant_owned(&state, &auth, &query.merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    let needle = query.q.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Json(
            json!({"transactions": [], "payment_links": [], "invoices": []}),
        ));
    }
    let transactions: Vec<Value> = state
        .transactions
        .read()
        .await
        .values()
        .filter(|t| t.merchant_id == query.merchant_id)
        .filter(|t| {
            t.id.to_lowercase().contains(&needle)
                || t.reference
                    .as_deref()
                    .map(|r| r.to_lowercase().contains(&needle))
                    .unwrap_or(false)
        })
        .take(20)
        .map(|t| json!({"id": t.id, "status": format!("{:?}", t.status), "amount_minor": t.amount.amount_minor}))
        .collect();
    let payment_links: Vec<Value> = state
        .payment_links
        .read()
        .await
        .values()
        .filter(|l| l.merchant_id == query.merchant_id)
        .filter(|l| {
            l.slug.to_lowercase().contains(&needle) || l.title.to_lowercase().contains(&needle)
        })
        .take(20)
        .map(|l| json!({"id": l.id, "slug": l.slug, "title": l.title}))
        .collect();
    let invoices: Vec<Value> = state
        .invoices
        .read()
        .await
        .values()
        .filter(|i| i.merchant_id == query.merchant_id)
        .filter(|i| {
            i.id.to_lowercase().contains(&needle) || i.number.to_lowercase().contains(&needle)
        })
        .take(20)
        .map(|i| json!({"id": i.id, "number": i.number, "status": format!("{:?}", i.status)}))
        .collect();
    Ok(Json(json!({
        "transactions": transactions,
        "payment_links": payment_links,
        "invoices": invoices,
    })))
}

// ---------------------------------------------------------------------
// Wave 108 — gateway test connection
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TestGatewayReq {
    pub merchant_id: String,
}

/// `POST /gateways/:provider/test`
pub async fn test_gateway(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Json(req): Json<TestGatewayReq>,
) -> Result<Json<Value>, ApiError> {
    if !merchant_owned(&state, &auth, &req.merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    let configs = state
        .gateway_configs
        .read()
        .await
        .get(&req.merchant_id)
        .cloned()
        .unwrap_or_default();
    let config = configs
        .iter()
        .find(|c| c.gateway.as_str() == provider.as_str())
        .cloned()
        .ok_or_else(|| {
            ApiError::BadRequest(format!("no {provider} configuration for this merchant"))
        })?;
    let transport = miraclepay_gateways::transport::ReqwestTransport::default();
    let detail = miraclepay_gateways::transport::test_connection(&transport, &config)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(
        json!({"ok": true, "provider": provider, "detail": detail}),
    ))
}

// ---------------------------------------------------------------------
// Wave 111 — payment link stats
// ---------------------------------------------------------------------

/// `GET /payment-links/:id/stats`
pub async fn link_stats(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(link_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let link = state
        .payment_links
        .read()
        .await
        .get(&link_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound("payment link".to_string()))?;
    if !merchant_owned(&state, &auth, &link.merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    let views = state
        .link_views
        .read()
        .await
        .get(&link_id)
        .copied()
        .unwrap_or(0);
    let (payments, completed) = {
        let link = &link;
        let txs = state.transactions.read().await;
        let mine: Vec<_> = txs
            .values()
            .filter(|t| {
                t.metadata.get("payment_link_slug").and_then(|v| v.as_str())
                    == Some(link.slug.as_str())
            })
            .collect();
        (
            mine.len(),
            mine.iter()
                .filter(|t| t.status == miraclepay_domain::transaction::TxStatus::Completed)
                .count(),
        )
    };
    Ok(Json(json!({
        "link_id": link_id,
        "views": views,
        "payments": payments,
        "completed_payments": completed,
        "conversion_percent": if views == 0 { 0 } else { completed as u64 * 100 / views.max(1) },
    })))
}

// ---------------------------------------------------------------------
// Wave 112 — receipt PDF
// ---------------------------------------------------------------------

/// `GET /transactions/:id/receipt.pdf`
pub async fn receipt_pdf(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(tx_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let tx = state
        .transactions
        .read()
        .await
        .get(&tx_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound("transaction".to_string()))?;
    if !merchant_owned(&state, &auth, &tx.merchant_id).await {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    let bytes = miraclepay_domain::invoice_pdf::render_receipt_pdf(&tx);
    Ok(([(header::CONTENT_TYPE, "application/pdf")], bytes))
}

// ---------------------------------------------------------------------
// Wave 113 — admin user management
// ---------------------------------------------------------------------

/// `GET /admin/users`
pub async fn admin_list_users(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&auth)?;
    let users: Vec<Value> = state
        .users
        .read()
        .await
        .values()
        .map(|u| {
            json!({
                "id": u.id, "name": u.name, "email": u.email,
                "role": format!("{:?}", u.role).to_lowercase(),
                "email_verified": u.email_verified, "suspended": u.suspended,
                "created_at": u.created_at,
            })
        })
        .collect();
    Ok(Json(json!({"users": users})))
}

async fn set_suspended(
    auth: &AuthUser,
    state: &AppState,
    user_id: &str,
    suspended: bool,
) -> Result<Json<Value>, ApiError> {
    require_admin(auth)?;
    if auth.user_id == user_id && suspended {
        return Err(ApiError::BadRequest(
            "an admin cannot suspend their own account".to_string(),
        ));
    }
    let mut users = state.users.write().await;
    let user = users
        .get_mut(user_id)
        .ok_or_else(|| ApiError::NotFound("user".to_string()))?;
    user.suspended = suspended;
    state.audit(
        Some(auth.user_id.clone()),
        None,
        if suspended {
            "admin.user_suspended"
        } else {
            "admin.user_activated"
        },
        "user",
        Some(user_id.to_string()),
    );
    Ok(Json(json!({"ok": true, "suspended": suspended})))
}

/// `POST /admin/users/:id/suspend`
pub async fn admin_suspend_user(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_suspended(&auth, &state, &user_id, true).await
}

/// `POST /admin/users/:id/activate`
pub async fn admin_activate_user(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_suspended(&auth, &state, &user_id, false).await
}

// Re-export so route wiring can scope-check API keys in one place.
pub fn payments_read_scope() -> ApiScope {
    ApiScope::PaymentsRead
}
