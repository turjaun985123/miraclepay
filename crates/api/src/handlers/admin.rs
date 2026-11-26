//! Admin handlers for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! Every handler in this module is platform-operator territory: the
//! first thing each one does is require the authenticated caller to
//! carry the `"admin"` role (see [`require_admin`]), and anything else
//! is rejected with [`ApiError::Forbidden`] before any state is read
//! or mutated. Merchants, staff, and customers never reach the data
//! below, even though they authenticate through the same middleware.
//!
//! The handlers are thin by design: aggregation is delegated to the
//! domain layer ([`miraclepay_domain::stats::compute_stats`]), records
//! are returned exactly as stored (a [`User`]'s `password_hash` is
//! skipped by its own `Serialize` implementation, so listing users
//! cannot leak it), and the only writes are a merchant's lifecycle
//! status and the platform settings — both recorded in the audit log.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use miraclepay_domain::audit::AuditEntry;
use miraclepay_domain::merchant::{Merchant, MerchantStatus};
use miraclepay_domain::money::Currency;
use miraclepay_domain::settings::{PlatformSettings, MAX_FEE_BPS};
use miraclepay_domain::stats::{compute_stats, DashboardStats};
use miraclepay_domain::user::User;

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

/// Default page size for the list endpoints when the caller does not
/// supply an explicit `limit` query parameter.
const DEFAULT_LIMIT: usize = 100;

/// Require the platform administrator role.
///
/// This is the single gate every handler in this module passes
/// through first: `auth.role` must be exactly `"admin"`, anything
/// else (merchant, staff, customer, or an unrecognised role string)
/// is [`ApiError::Forbidden`].
fn require_admin(auth: &AuthUser) -> Result<(), ApiError> {
    if auth.role == "admin" {
        Ok(())
    } else {
        Err(ApiError::Forbidden("forbidden".to_string()))
    }
}

/// Query parameters shared by the list endpoints: an optional maximum
/// number of records to return.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LimitQuery {
    /// Maximum number of records to return; defaults to
    /// [`DEFAULT_LIMIT`] when omitted.
    pub limit: Option<usize>,
}

impl LimitQuery {
    /// The effective limit, applying the default when unset.
    fn effective_limit(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_LIMIT)
    }
}

/// Response body for [`platform_stats`]: the platform-wide dashboard
/// statistics plus the headline record counts that are not part of
/// [`DashboardStats`] itself.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformStats {
    /// Transaction / refund aggregates in BDT, from
    /// [`compute_stats`].
    #[serde(flatten)]
    pub stats: DashboardStats,
    /// Total number of registered user accounts.
    pub total_users: u64,
    /// Total number of registered merchants.
    pub total_merchants: u64,
}

/// `GET /admin/stats` — platform-wide statistics for administrators.
///
/// Aggregates every transaction and refund on the platform in BDT
/// (the platform's reporting currency; amounts in other currencies
/// are excluded from the money totals by [`compute_stats`], while
/// status counts cover all transactions) and adds the total numbers
/// of user accounts and merchants.
pub async fn platform_stats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<PlatformStats>, ApiError> {
    require_admin(&auth)?;

    let transactions = state.transactions.read().await;
    let refunds = state.refunds.read().await;
    let txs: Vec<_> = transactions.values().cloned().collect();
    let rfs: Vec<_> = refunds.values().cloned().collect();
    let stats = compute_stats(&txs, &rfs, Currency::BDT);
    let total_users = state.users.read().await.len() as u64;
    let total_merchants = state.merchants.read().await.len() as u64;

    Ok(Json(PlatformStats {
        stats,
        total_users,
        total_merchants,
    }))
}

/// `POST /admin/notifications/dispatch` (Wave 35) — hand every
/// queued notification to the SMTP transport and record outcomes.
/// Without SMTP configuration this is a 409 with the missing
/// variables named: queued mail is never fake-sent.
pub async fn dispatch_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let config = miraclepay_worker::smtp_transport::SmtpConfig::from_env()
        .map_err(ApiError::Conflict)?;
    let transport = miraclepay_worker::smtp_transport::SmtpTransport { config };
    let mut notifications = state.notifications.write().await;
    let summary =
        miraclepay_worker::notification_job::dispatch_batch(&transport, &mut notifications).await;
    Ok(Json(serde_json::json!({
        "sent": summary.sent,
        "failed": summary.failed,
    })))
}

/// `GET /admin/fraud/flagged` (Wave 39) — transactions whose risk
/// assessment is medium or high (score ≥ 30), highest first,
/// paginated. Assessments are advisory: nothing is auto-blocked.
pub async fn fraud_flagged(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(page): Query<crate::pagination::PageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let assessments = state.risk_assessments.read().await;
    let transactions = state.transactions.read().await;
    let mut flagged: Vec<serde_json::Value> = assessments
        .iter()
        .filter(|(_, assessment)| assessment.score >= 30)
        .map(|(tx_id, assessment)| {
            let tx = transactions.get(tx_id);
            serde_json::json!({
                "transaction_id": tx_id,
                "merchant_id": tx.map(|t| t.merchant_id.clone()),
                "amount_minor": tx.map(|t| t.amount.amount_minor),
                "currency": tx.map(|t| t.amount.currency.code()),
                "status": tx.map(|t| format!("{:?}", t.status)),
                "score": assessment.score,
                "level": assessment.level,
                "signals": assessment.signals,
            })
        })
        .collect();
    flagged.sort_by_key(|entry| {
        std::cmp::Reverse(entry["score"].as_u64().unwrap_or(0))
    });
    let total = flagged.len();
    let items = crate::pagination::apply_page(flagged, page.page, page.per_page);
    Ok(Json(serde_json::json!({ "items": items, "total": total })))
}

/// Query for [`platform_analytics`].
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct AnalyticsQuery {
    /// Days of daily series to return (clamped to 1..=90, default 30).
    pub days: Option<u32>,
}

/// `GET /admin/analytics` — platform analytics for administrators
/// (Wave 19): a zero-filled daily series (counts + completed BDT
/// volume) and a per-gateway breakdown, computed by
/// `miraclepay_domain::analytics`.
pub async fn platform_analytics(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let days = query.days.unwrap_or(30).clamp(1, 90);
    let transactions = state.transactions.read().await;
    let txs: Vec<_> = transactions.values().cloned().collect();
    drop(transactions);
    let daily = miraclepay_domain::analytics::daily_series(&txs, days, Utc::now());
    let gateways = miraclepay_domain::analytics::gateway_breakdown(&txs);
    Ok(Json(serde_json::json!({
        "days": days,
        "daily": daily,
        "gateways": gateways,
    })))
}

/// `GET /admin/users` — list user accounts, most of the platform's
/// users up to `limit`.
///
/// The records are returned as stored; password hashes cannot leak
/// through this endpoint because [`User`]'s `Serialize` implementation
/// skips the `password_hash` field entirely.
pub async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<User>>, ApiError> {
    require_admin(&auth)?;

    let users = state.users.read().await;
    let page: Vec<User> = users.values().take(query.effective_limit()).cloned().collect();
    Ok(Json(page))
}

/// `GET /admin/merchants` — list every merchant on the platform,
/// regardless of lifecycle status.
pub async fn list_all_merchants(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<Merchant>>, ApiError> {
    require_admin(&auth)?;

    let merchants = state.merchants.read().await;
    Ok(Json(merchants.values().cloned().collect()))
}

/// Request body for [`set_merchant_status`]: the desired lifecycle
/// status as its snake-case name (`"pending"`, `"active"`,
/// `"suspended"`, or `"closed"`).
#[derive(Debug, Clone, Deserialize)]
pub struct StatusReq {
    /// New status for the merchant, by name.
    pub status: String,
}

/// Parse a merchant status name, rejecting anything that is not one
/// of the four lifecycle states with [`ApiError::BadRequest`].
fn parse_merchant_status(raw: &str) -> Result<MerchantStatus, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pending" => Ok(MerchantStatus::Pending),
        "active" => Ok(MerchantStatus::Active),
        "suspended" => Ok(MerchantStatus::Suspended),
        "closed" => Ok(MerchantStatus::Closed),
        other => Err(ApiError::BadRequest(format!(
            "unknown merchant status {other:?}: expected \"pending\", \"active\", \
             \"suspended\", or \"closed\""
        ))),
    }
}

/// `POST /admin/merchants/{id}/status` — set a merchant's lifecycle
/// status (approve, suspend, close, or return to pending).
///
/// The change is written to the audit log with the acting admin, the
/// previous status, and the new status. An unknown merchant id is
/// [`ApiError::NotFound`]; an unrecognised status name is
/// [`ApiError::BadRequest`] and changes nothing.
pub async fn set_merchant_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<StatusReq>,
) -> Result<Json<Merchant>, ApiError> {
    require_admin(&auth)?;
    let new_status = parse_merchant_status(&req.status)?;

    // Wave 33: a merchant whose KYC was rejected cannot be activated.
    if new_status == miraclepay_domain::merchant::MerchantStatus::Active {
        let profiles = state.kyc_profiles.read().await;
        if let Some(profile) = profiles.get(&id) {
            if profile.status == miraclepay_domain::kyc::KycStatus::Rejected {
                return Err(ApiError::Conflict(
                    "merchant KYC is rejected; the merchant must resubmit before activation"
                        .to_string(),
                ));
            }
        }
    }

    let (previous_status, updated) = {
        let mut merchants = state.merchants.write().await;
        let merchant = merchants
            .values_mut()
            .find(|merchant| merchant.id == id)
            .ok_or_else(|| ApiError::NotFound(format!("merchant {id} not found")))?;
        let previous_status = merchant.status;
        merchant.status = new_status;
        (previous_status, merchant.clone())
    };

    state
        .audit_log
        .write()
        .await
        .push(AuditEntry {
            id: miraclepay_domain::ids::audit_id(),
            actor_user_id: Some(auth.user_id.clone()),
            merchant_id: Some(id.clone()),
            action: "merchant.status_changed".to_string(),
            entity_type: "merchant".to_string(),
            entity_id: Some(id),
            ip_address: None,
            user_agent: None,
            details: serde_json::json!({
                "from": serde_json::to_value(previous_status).unwrap_or(serde_json::Value::Null),
                "to": serde_json::to_value(new_status).unwrap_or(serde_json::Value::Null),
            }),
            created_at: Utc::now(),
        });

    Ok(Json(updated))
}

/// Query parameters for [`list_audit`] (Wave 13): the legacy
/// `limit` cap plus exact-match filters and Wave 11 paging.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct AuditQuery {
    /// Maximum number of records to return (legacy behaviour, used
    /// when no paging parameters are supplied).
    pub limit: Option<usize>,
    /// Only entries whose action starts with this prefix
    /// (e.g. `merchant.` matches `merchant.created`).
    pub action: Option<String>,
    /// Only entries for this entity type (e.g. `merchant`).
    pub entity_type: Option<String>,
    /// Only entries attributed to this actor (user id).
    pub actor_user_id: Option<String>,
    /// Wave 11 paging: 1-based page number.
    pub page: Option<u32>,
    /// Wave 11 paging: page size (clamped to 1..=100).
    pub per_page: Option<u32>,
}

/// `GET /admin/audit` — read the platform audit log, newest first.
///
/// Entries are stored oldest first, so this returns them in reverse
/// (most recent action first). Wave 13 adds `action` (prefix),
/// `entity_type` and `actor_user_id` filters plus `page`/`per_page`
/// paging; without paging parameters the result is capped at
/// `limit` as before.
pub async fn list_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>, ApiError> {
    require_admin(&auth)?;

    let audit = state.audit_log.read().await;
    let mut entries: Vec<AuditEntry> = audit
        .iter()
        .rev()
        .filter(|e| match query.action.as_deref() {
            Some(prefix) => e.action.starts_with(prefix),
            None => true,
        })
        .filter(|e| match query.entity_type.as_deref() {
            Some(t) => e.entity_type == t,
            None => true,
        })
        .filter(|e| match query.actor_user_id.as_deref() {
            Some(actor) => e.actor_user_id.as_deref() == Some(actor),
            None => true,
        })
        .cloned()
        .collect();
    if query.page.is_some() || query.per_page.is_some() {
        entries = crate::pagination::apply_page(entries, query.page, query.per_page);
    } else {
        entries.truncate(query.limit.unwrap_or(DEFAULT_LIMIT));
    }
    Ok(Json(entries))
}

/// `GET /admin/settings` — read the current platform settings.
pub async fn get_settings(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<PlatformSettings>, ApiError> {
    require_admin(&auth)?;

    let settings = state.settings.read().await;
    Ok(Json(settings.clone()))
}

/// `PUT /admin/settings` — replace the platform settings wholesale.
///
/// The replacement is validated before it is stored: a
/// `default_fee_bps` above 10_000 (100%) is
/// [`ApiError::BadRequest`] and leaves the current settings
/// untouched. A successful replacement is written to the audit log.
pub async fn update_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(new_settings): Json<PlatformSettings>,
) -> Result<Json<PlatformSettings>, ApiError> {
    require_admin(&auth)?;

    if new_settings.default_fee_bps > MAX_FEE_BPS {
        return Err(ApiError::BadRequest(format!(
            "default_fee_bps must be at most {MAX_FEE_BPS} (100%), got {}",
            new_settings.default_fee_bps
        )));
    }

    {
        let mut settings = state.settings.write().await;
        *settings = new_settings.clone();
    }

    state
        .audit_log
        .write()
        .await
        .push(AuditEntry {
            id: miraclepay_domain::ids::audit_id(),
            actor_user_id: Some(auth.user_id.clone()),
            merchant_id: None,
            action: miraclepay_domain::audit::AuditAction::SettingsUpdated
                .as_str()
                .to_string(),
            entity_type: "settings".to_string(),
            entity_id: None,
            ip_address: None,
            user_agent: None,
            details: serde_json::json!({
                "default_fee_bps": new_settings.default_fee_bps,
                "maintenance_mode": new_settings.maintenance_mode,
            }),
            created_at: Utc::now(),
        });

    Ok(Json(new_settings))
}
