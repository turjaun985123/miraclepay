//! Database wiring for the MiraclePay API (Wave 2).
//!
//! Storage modes:
//! - `DATABASE_URL` **unset** → in-memory store (the v0.0.1 default).
//!   The API logs this clearly at startup; it never panics.
//! - `DATABASE_URL` **set** → a Postgres pool is created and the
//!   workspace migrations (`migrations/0001..0004`) are applied. If
//!   the connection or the migrations fail, the error is logged and
//!   the API falls back to the in-memory store rather than crashing —
//!   a misconfigured deploy stays up and observable via `/health`.
//!
//! The pool is exposed on [`crate::AppState`] (`db_pool`) and its
//! mode via `/health`'s `db` field (`"postgres"` / `"in_memory"`).
//! Handlers migrate to SQLx queries incrementally per wave; the
//! in-memory collections remain the fallback path.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Which storage backend the API is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbMode {
    /// No `DATABASE_URL` (or connection failed): in-memory store.
    InMemory,
    /// Connected Postgres pool with migrations applied.
    Postgres,
}

impl DbMode {
    /// Stable string used in logs and the `/health` payload.
    pub fn as_str(&self) -> &'static str {
        match self {
            DbMode::InMemory => "in_memory",
            DbMode::Postgres => "postgres",
        }
    }
}

/// Decide the intended mode from an optional `DATABASE_URL` value.
///
/// Pure function (no I/O) so the decision is unit-testable: an unset,
/// empty, or whitespace-only value means in-memory; anything else is
/// treated as a Postgres URL.
pub fn intended_mode(database_url: Option<&str>) -> DbMode {
    match database_url {
        Some(url) if !url.trim().is_empty() => DbMode::Postgres,
        _ => DbMode::InMemory,
    }
}

/// Result of database initialisation: the mode actually in effect
/// (after any fallback) plus the pool when Postgres is live.
pub struct DbInit {
    pub mode: DbMode,
    pub pool: Option<PgPool>,
}

/// Initialise the database from the `DATABASE_URL` environment
/// variable. Never panics: every failure path logs and falls back
/// to [`DbMode::InMemory`].
pub async fn init_from_env() -> DbInit {
    let url = std::env::var("DATABASE_URL").ok();
    init(url.as_deref()).await
}

/// Initialise from an explicit optional URL (used by `init_from_env`
/// and by tests).
pub async fn init(database_url: Option<&str>) -> DbInit {
    if intended_mode(database_url) == DbMode::InMemory {
        eprintln!(
            "[miraclepay] DATABASE_URL not set — running with the in-memory store \
             (data will not persist across restarts)"
        );
        return DbInit { mode: DbMode::InMemory, pool: None };
    }
    let url = database_url.expect("intended_mode checked non-empty");
    match connect_and_migrate(url).await {
        Ok(pool) => {
            eprintln!("[miraclepay] Postgres connected; migrations applied");
            DbInit { mode: DbMode::Postgres, pool: Some(pool) }
        }
        Err(err) => {
            eprintln!(
                "[miraclepay] Postgres init failed ({err}) — falling back to the \
                 in-memory store; set a valid DATABASE_URL to persist data"
            );
            DbInit { mode: DbMode::InMemory, pool: None }
        }
    }
}

async fn connect_and_migrate(url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_url_means_in_memory() {
        assert_eq!(intended_mode(None), DbMode::InMemory);
    }

    #[test]
    fn empty_or_blank_url_means_in_memory() {
        assert_eq!(intended_mode(Some("")), DbMode::InMemory);
        assert_eq!(intended_mode(Some("   ")), DbMode::InMemory);
    }

    #[test]
    fn real_url_means_postgres() {
        assert_eq!(
            intended_mode(Some("postgres://user:pass@localhost:5432/miraclepay")),
            DbMode::Postgres
        );
    }

    #[tokio::test]
    async fn init_without_url_falls_back_without_panic() {
        let init = init(None).await;
        assert_eq!(init.mode, DbMode::InMemory);
        assert!(init.pool.is_none());
    }

    #[tokio::test]
    async fn init_with_unreachable_url_falls_back_without_panic() {
        // Port 1 on loopback is closed; acquire timeout keeps this fast.
        let init = init(Some("postgres://u:p@127.0.0.1:1/nope")).await;
        assert_eq!(init.mode, DbMode::InMemory);
        assert!(init.pool.is_none());
    }
}
