//! Postgres persistence for the API collections (Waves 53–75).
//!
//! v0.0.1 kept every collection in memory: the Postgres pool
//! connected and migrations ran, but handlers never persisted.
//! v0.0.2 closes that gap with a record-store layer:
//!
//! - Every [`AppState`] collection is mirrored into the
//!   `mp_records` table (migration 0005) as JSONB, one row per
//!   record, keyed by `(collection, id)`.
//! - [`snapshot`] writes the full state atomically: one transaction
//!   replaces each collection's rows, so a crash can never leave a
//!   half-written collection. A background task snapshots
//!   periodically (see [`spawn_snapshot_task`]); the interval is
//!   `PERSIST_SNAPSHOT_SECS` (default 15, `0` disables the task).
//! - [`hydrate`] loads the store back into a fresh [`AppState`] at
//!   startup, before any request is served — a restart against the
//!   same database picks up exactly where the last snapshot left
//!   off.
//! - With no pool (in-memory mode) every function here is a no-op:
//!   the fallback behaviour of v0.0.1 is unchanged.
//!
//! The per-collection mapping lives entirely in this module
//! ([`collect`] / [`apply`]); handlers keep working against the
//! in-memory collections and never see SQL. Collections covered:
//! users, merchants, api_keys (SMS devices), gateway_configs,
//! settings (platform settings incl. FX rates), transactions,
//! payment_links, invoices, webhook_events, deliveries, plans,
//! subscriptions, refunds, settlements, disputes, notifications,
//! audit_log, used_trx, risk_assessments, kyc_profiles,
//! merchant_settings, refresh_tokens.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::PgPool;

use crate::state::AppState;

/// One collection's records as `(id, json)` pairs.
type CollectionData = Vec<(String, serde_json::Value)>;

/// All collection names mirrored into `mp_records`.
pub const COLLECTIONS: &[&str] = &[
    "users",
    "merchants",
    "api_keys",
    "gateway_configs",
    "settings",
    "transactions",
    "payment_links",
    "invoices",
    "webhook_events",
    "deliveries",
    "plans",
    "subscriptions",
    "refunds",
    "settlements",
    "disputes",
    "notifications",
    "audit_log",
    "used_trx",
    "risk_assessments",
    "kyc_profiles",
    "merchant_settings",
    "refresh_tokens",
    "auth_tokens",
];

fn to_value<T: Serialize>(item: &T) -> serde_json::Value {
    serde_json::to_value(item).unwrap_or(serde_json::Value::Null)
}

fn map_entries<T: Serialize, M: std::ops::Deref<Target = HashMap<String, T>>>(
    map: &M,
) -> CollectionData {
    map.iter()
        .map(|(id, item)| (id.clone(), to_value(item)))
        .collect()
}

fn vec_entries<T: Serialize, V: std::ops::Deref<Target = Vec<T>>>(items: &V) -> CollectionData {
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let value = to_value(item);
            let id = value
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("row-{idx}"));
            (id, value)
        })
        .collect()
}

/// Persistence representation of a user.
///
/// The API-facing [`User`] serialization deliberately omits
/// `password_hash` and `totp_secret` so hashes never leak into
/// responses — but the record store must keep them, or a restart
/// would lock every user out. This repr is used only inside
/// `mp_records` (never in an HTTP response).
#[derive(serde::Serialize, serde::Deserialize)]
struct PersistUser {
    id: String,
    name: String,
    email: String,
    password_hash: String,
    role: miraclepay_domain::user::UserRole,
    email_verified: bool,
    totp_secret: Option<String>,
    totp_enabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<&miraclepay_domain::user::User> for PersistUser {
    fn from(u: &miraclepay_domain::user::User) -> Self {
        Self {
            id: u.id.clone(),
            name: u.name.clone(),
            email: u.email.clone(),
            password_hash: u.password_hash.clone(),
            role: u.role,
            email_verified: u.email_verified,
            totp_secret: u.totp_secret.clone(),
            totp_enabled: u.totp_enabled,
            created_at: u.created_at,
        }
    }
}

impl From<PersistUser> for miraclepay_domain::user::User {
    fn from(u: PersistUser) -> Self {
        Self {
            id: u.id,
            name: u.name,
            email: u.email,
            password_hash: u.password_hash,
            role: u.role,
            email_verified: u.email_verified,
            totp_secret: u.totp_secret,
            totp_enabled: u.totp_enabled,
            created_at: u.created_at,
        }
    }
}

/// Gather every collection out of the live state as JSON records.
/// This is the single place that knows the collection list on the
/// write path; [`apply`] is its mirror on the read path.
pub async fn collect(state: &AppState) -> Vec<(&'static str, CollectionData)> {
    let mut out: Vec<(&'static str, CollectionData)> = Vec::with_capacity(COLLECTIONS.len());
    out.push((
        "users",
        state
            .users
            .read()
            .await
            .iter()
            .map(|(id, u)| (id.clone(), to_value(&PersistUser::from(u))))
            .collect(),
    ));
    out.push(("merchants", map_entries(&state.merchants.read().await)));
    out.push(("api_keys", map_entries(&state.api_keys.read().await)));
    out.push((
        "gateway_configs",
        map_entries(&state.gateway_configs.read().await),
    ));
    out.push((
        "settings",
        vec![(
            "platform".to_string(),
            to_value(&*state.settings.read().await),
        )],
    ));
    out.push((
        "transactions",
        map_entries(&state.transactions.read().await),
    ));
    out.push((
        "payment_links",
        map_entries(&state.payment_links.read().await),
    ));
    out.push(("invoices", map_entries(&state.invoices.read().await)));
    out.push((
        "webhook_events",
        vec_entries(&state.webhook_events.read().await),
    ));
    out.push(("deliveries", vec_entries(&state.deliveries.read().await)));
    out.push(("plans", map_entries(&state.plans.read().await)));
    out.push((
        "subscriptions",
        map_entries(&state.subscriptions.read().await),
    ));
    out.push(("refunds", map_entries(&state.refunds.read().await)));
    out.push(("settlements", map_entries(&state.settlements.read().await)));
    out.push(("disputes", map_entries(&state.disputes.read().await)));
    out.push((
        "notifications",
        vec_entries(&state.notifications.read().await),
    ));
    out.push(("audit_log", vec_entries(&state.audit_log.read().await)));
    out.push((
        "used_trx",
        state
            .used_trx
            .read()
            .await
            .iter()
            .map(|trx| (trx.clone(), serde_json::Value::String(trx.clone())))
            .collect(),
    ));
    out.push((
        "risk_assessments",
        map_entries(&state.risk_assessments.read().await),
    ));
    out.push((
        "kyc_profiles",
        map_entries(&state.kyc_profiles.read().await),
    ));
    out.push((
        "merchant_settings",
        map_entries(&state.merchant_settings.read().await),
    ));
    out.push((
        "refresh_tokens",
        map_entries(&state.refresh_tokens.read().await),
    ));
    out.push(("auth_tokens", map_entries(&state.auth_tokens.read().await)));
    out
}

/// Write the full state into `mp_records`, atomically per snapshot.
/// Returns the number of records written. No-op (Ok(0)) in
/// in-memory mode.
pub async fn snapshot(state: &AppState) -> Result<usize, sqlx::Error> {
    let Some(pool) = &state.db_pool else {
        return Ok(0);
    };
    snapshot_to(pool, state).await
}

/// [`snapshot`] against an explicit pool (used by tests).
pub async fn snapshot_to(pool: &PgPool, state: &AppState) -> Result<usize, sqlx::Error> {
    let collections = collect(state).await;
    let mut tx = pool.begin().await?;
    let mut written = 0usize;
    for (name, entries) in &collections {
        sqlx::query("DELETE FROM mp_records WHERE collection = $1")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        for chunk in entries.chunks(200) {
            let mut builder =
                sqlx::QueryBuilder::new("INSERT INTO mp_records (collection, id, data) ");
            builder.push_values(chunk, |mut b, (id, data)| {
                b.push_bind(name).push_bind(id).push_bind(data);
            });
            builder.build().execute(&mut *tx).await?;
            written += chunk.len();
        }
    }
    tx.commit().await?;
    Ok(written)
}

/// Load every persisted collection into `state`, replacing the
/// (empty-at-startup) in-memory collections. Returns the number of
/// records loaded. No-op (Ok(0)) in in-memory mode.
pub async fn hydrate(state: &AppState) -> Result<usize, sqlx::Error> {
    let Some(pool) = &state.db_pool else {
        return Ok(0);
    };
    hydrate_from(pool, state).await
}

/// [`hydrate`] against an explicit pool (used by tests).
pub async fn hydrate_from(pool: &PgPool, state: &AppState) -> Result<usize, sqlx::Error> {
    let rows: Vec<(String, String, serde_json::Value)> =
        sqlx::query_as("SELECT collection, id, data FROM mp_records ORDER BY collection, id")
            .fetch_all(pool)
            .await?;
    let total = rows.len();
    let mut grouped: HashMap<String, CollectionData> = HashMap::new();
    for (collection, id, data) in rows {
        grouped.entry(collection).or_default().push((id, data));
    }
    for (name, entries) in &grouped {
        apply(state, name, entries).await;
    }
    Ok(total)
}

fn from_value<T: serde::de::DeserializeOwned>(value: &serde_json::Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}

async fn apply_map<T: serde::de::DeserializeOwned>(
    target: &tokio::sync::RwLock<HashMap<String, T>>,
    entries: &CollectionData,
) {
    let mut map = target.write().await;
    for (id, value) in entries {
        if let Some(item) = from_value::<T>(value) {
            map.insert(id.clone(), item);
        }
    }
}

async fn apply_vec<T: serde::de::DeserializeOwned>(
    target: &tokio::sync::RwLock<Vec<T>>,
    entries: &CollectionData,
) {
    let mut items: Vec<(String, T)> = entries
        .iter()
        .filter_map(|(id, value)| from_value::<T>(value).map(|item| (id.clone(), item)))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut vec = target.write().await;
    vec.extend(items.into_iter().map(|(_, item)| item));
}

/// Mirror of [`collect`]: load one collection's records into state.
/// Unknown collections are ignored (forward compatibility with
/// stores written by newer versions).
pub async fn apply(state: &AppState, collection: &str, entries: &CollectionData) {
    match collection {
        "users" => {
            let mut map = state.users.write().await;
            for (id, value) in entries {
                if let Some(u) = from_value::<PersistUser>(value) {
                    map.insert(id.clone(), u.into());
                }
            }
        }
        "merchants" => apply_map(&state.merchants, entries).await,
        "api_keys" => apply_map(&state.api_keys, entries).await,
        "gateway_configs" => apply_map(&state.gateway_configs, entries).await,
        "settings" => {
            if let Some((_, value)) = entries.iter().find(|(id, _)| id == "platform") {
                if let Some(settings) = from_value(value) {
                    *state.settings.write().await = settings;
                }
            }
        }
        "transactions" => apply_map(&state.transactions, entries).await,
        "payment_links" => apply_map(&state.payment_links, entries).await,
        "invoices" => apply_map(&state.invoices, entries).await,
        "webhook_events" => apply_vec(&state.webhook_events, entries).await,
        "deliveries" => apply_vec(&state.deliveries, entries).await,
        "plans" => apply_map(&state.plans, entries).await,
        "subscriptions" => apply_map(&state.subscriptions, entries).await,
        "refunds" => apply_map(&state.refunds, entries).await,
        "settlements" => apply_map(&state.settlements, entries).await,
        "disputes" => apply_map(&state.disputes, entries).await,
        "notifications" => apply_vec(&state.notifications, entries).await,
        "audit_log" => apply_vec(&state.audit_log, entries).await,
        "used_trx" => {
            let mut set = state.used_trx.write().await;
            for (_, value) in entries {
                if let Some(trx) = value.as_str() {
                    set.insert(trx.to_string());
                }
            }
        }
        "risk_assessments" => apply_map(&state.risk_assessments, entries).await,
        "kyc_profiles" => apply_map(&state.kyc_profiles, entries).await,
        "merchant_settings" => apply_map(&state.merchant_settings, entries).await,
        "refresh_tokens" => apply_map(&state.refresh_tokens, entries).await,
        "auth_tokens" => apply_map(&state.auth_tokens, entries).await,
        _ => {}
    }
}

/// Row counts per collection currently in `mp_records`
/// (collection, rows), for `/health` and diagnostics.
pub async fn persisted_stats(pool: &PgPool) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT collection, COUNT(*) FROM mp_records GROUP BY collection ORDER BY collection",
    )
    .fetch_all(pool)
    .await
}

/// Spawn the periodic snapshot task (Wave 71). Interval from
/// `PERSIST_SNAPSHOT_SECS` (default 15; `0` disables). Errors are
/// logged, never fatal: the next tick retries.
pub fn spawn_snapshot_task(state: AppState) {
    if state.db_pool.is_none() {
        return;
    }
    let secs: u64 = std::env::var("PERSIST_SNAPSHOT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15);
    if secs == 0 {
        eprintln!("[miraclepay] persistence snapshots disabled (PERSIST_SNAPSHOT_SECS=0)");
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(secs));
        ticker.tick().await; // consume the immediate first tick
        loop {
            ticker.tick().await;
            match snapshot(&state).await {
                Ok(n) => eprintln!("[miraclepay] persistence snapshot: {n} records"),
                Err(err) => eprintln!("[miraclepay] persistence snapshot failed: {err}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refresh::RefreshRecord;
    use miraclepay_domain::user::{User, UserRole};

    fn sample_user(id: &str) -> User {
        User {
            id: id.to_string(),
            name: "Test".to_string(),
            email: format!("{id}@example.com"),
            password_hash: "x".to_string(),
            role: UserRole::Merchant,
            email_verified: false,
            totp_secret: None,
            totp_enabled: false,
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn collect_covers_every_declared_collection() {
        let state = AppState::new("s".to_string());
        let collections = collect(&state).await;
        assert_eq!(collections.len(), COLLECTIONS.len());
        for (name, _) in &collections {
            assert!(COLLECTIONS.contains(name), "undeclared collection {name}");
        }
    }

    #[tokio::test]
    async fn collect_apply_round_trip_restores_records() {
        let source = AppState::new("s".to_string());
        source
            .users
            .write()
            .await
            .insert("usr_1".to_string(), sample_user("usr_1"));
        source.used_trx.write().await.insert("TRX123".to_string());
        source.refresh_tokens.write().await.insert(
            "hash1".to_string(),
            RefreshRecord {
                user_id: "usr_1".to_string(),
                expires_at: chrono::Utc::now(),
                revoked: false,
            },
        );

        let target = AppState::new("s".to_string());
        for (name, entries) in collect(&source).await {
            apply(&target, name, &entries).await;
        }
        assert_eq!(target.users.read().await.len(), 1);
        assert!(target.used_trx.read().await.contains("TRX123"));
        assert_eq!(target.refresh_tokens.read().await.len(), 1);
    }

    #[tokio::test]
    async fn snapshot_and_hydrate_are_noops_without_pool() {
        let state = AppState::new("s".to_string());
        assert_eq!(snapshot(&state).await.unwrap(), 0);
        assert_eq!(hydrate(&state).await.unwrap(), 0);
    }
}
