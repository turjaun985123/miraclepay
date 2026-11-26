//! Shared application state for the MiraclePay API (v0.0.1).
//!
//! Original clean-room implementation.
//!
//! [`AppState`] is the single, canonical store every HTTP handler
//! works against. In v0.0.1 the platform keeps its records in
//! memory: each collection lives behind its own asynchronous lock
//! inside an [`Arc`], so the state is cheap to clone into every
//! handler (cloning an [`AppState`] clones the `Arc`s, never the
//! data) while still letting concurrent requests read and write
//! different collections independently.
//!
//! Two kinds of collections are used:
//!
//! - keyed maps (`HashMap<String, _>`), for records that are looked
//!   up by their own identifier — users, merchants, transactions,
//!   payment links, invoices, refunds, settlements, subscription
//!   plans and subscriptions, API keys (keyed by the *hash* of the
//!   key, never by the raw key), and each merchant's gateway
//!   configurations (keyed by merchant id);
//! - append-only logs (`Vec<_>`), for records that are written in
//!   order and read back as a history — webhook events, webhook
//!   deliveries, notifications, and the audit log.
//!
//! The helpers on [`AppState`] cover the cross-cutting operations
//! handlers would otherwise each re-implement: recording an audit
//! entry ([`AppState::audit`]), building the gateway registry for
//! one merchant ([`AppState::registry_for`]), and seeding the first
//! administrator account ([`AppState::seed_admin`]).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;

use miraclepay_domain::audit::AuditEntry;
use miraclepay_domain::user::{User, UserRole};
use miraclepay_domain::{api_key, audit, ids, invoice, merchant, notification};
use miraclepay_domain::{payment_link, refund, settings, settlement, subscription};
use miraclepay_domain::{transaction, webhook};
use miraclepay_gateways::registry::GatewayRegistry;
use miraclepay_gateways::types::GatewayConfig;

/// The canonical shared state of the MiraclePay API.
///
/// See the module documentation for the storage model. Every field
/// is public: handlers are expected to lock the one collection they
/// need, do their work, and drop the guard promptly — never hold
/// two write guards at once, and never hold a guard across a call
/// into another handler.
#[derive(Clone)]
pub struct AppState {
    /// All registered user accounts, keyed by user id.
    pub users: Arc<tokio::sync::RwLock<HashMap<String, miraclepay_domain::user::User>>>,
    /// All merchant businesses, keyed by merchant id.
    pub merchants: Arc<RwLock<HashMap<String, merchant::Merchant>>>,
    /// All payment transactions, keyed by transaction id.
    pub transactions: Arc<RwLock<HashMap<String, transaction::Transaction>>>,
    /// All payment links, keyed by payment-link id.
    pub payment_links: Arc<RwLock<HashMap<String, payment_link::PaymentLink>>>,
    /// All invoices, keyed by invoice id.
    pub invoices: Arc<RwLock<HashMap<String, invoice::Invoice>>>,
    /// All refunds, keyed by refund id.
    pub refunds: Arc<RwLock<HashMap<String, refund::Refund>>>,
    /// All settlements, keyed by settlement id.
    pub settlements: Arc<RwLock<HashMap<String, settlement::Settlement>>>,
    /// All subscription plans, keyed by plan id.
    pub plans: Arc<RwLock<HashMap<String, subscription::Plan>>>,
    /// All subscriptions, keyed by subscription id.
    pub subscriptions: Arc<RwLock<HashMap<String, subscription::Subscription>>>,
    /// All API keys, keyed by the hash of the key (`key_hash`).
    ///
    /// The raw key is never used as a map key and never stored in
    /// this map, so a dump of the state cannot leak usable keys.
    pub api_keys: Arc<RwLock<HashMap<String, api_key::ApiKey>>>,
    /// Gateway configurations per merchant, keyed by merchant id.
    ///
    /// A merchant may configure several gateways, hence the `Vec`;
    /// at most one entry per gateway is kept inside it.
    pub gateway_configs: Arc<RwLock<HashMap<String, Vec<GatewayConfig>>>>,
    /// Append-only log of webhook events, oldest first.
    pub webhook_events: Arc<RwLock<Vec<webhook::WebhookEvent>>>,
    /// Append-only log of webhook delivery attempts, oldest first.
    pub deliveries: Arc<RwLock<Vec<webhook::WebhookDelivery>>>,
    /// Append-only log of notifications, oldest first.
    pub notifications: Arc<RwLock<Vec<notification::Notification>>>,
    /// Append-only audit log, oldest first.
    pub audit_log: Arc<RwLock<Vec<audit::AuditEntry>>>,
    /// Provider transaction references already consumed by the SMS
    /// verification flow.
    ///
    /// A reference is inserted the moment it settles a transaction,
    /// so the same provider reference can never settle two
    /// transactions (replay protection).
    pub used_trx: Arc<RwLock<HashSet<String>>>,
    /// Platform-wide settings, replaced wholesale by administrators.
    pub settings: Arc<RwLock<settings::PlatformSettings>>,
    /// Secret used to sign and verify session tokens (JWTs).
    pub jwt_secret: String,
    /// Postgres pool when `DATABASE_URL` was set and migrations
    /// succeeded (Wave 2); `None` means the in-memory store is the
    /// active backend. See [`crate::db`].
    pub db_pool: Option<sqlx::PgPool>,
}

impl AppState {
    /// Create a fresh, empty state.
    ///
    /// Every collection starts empty and the platform settings
    /// start at [`settings::PlatformSettings::default`]; the only
    /// caller-supplied value is the token-signing secret.
    pub fn new(jwt_secret: String) -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            merchants: Arc::new(RwLock::new(HashMap::new())),
            transactions: Arc::new(RwLock::new(HashMap::new())),
            payment_links: Arc::new(RwLock::new(HashMap::new())),
            invoices: Arc::new(RwLock::new(HashMap::new())),
            refunds: Arc::new(RwLock::new(HashMap::new())),
            settlements: Arc::new(RwLock::new(HashMap::new())),
            plans: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            api_keys: Arc::new(RwLock::new(HashMap::new())),
            gateway_configs: Arc::new(RwLock::new(HashMap::new())),
            webhook_events: Arc::new(RwLock::new(Vec::new())),
            deliveries: Arc::new(RwLock::new(Vec::new())),
            notifications: Arc::new(RwLock::new(Vec::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            used_trx: Arc::new(RwLock::new(HashSet::new())),
            settings: Arc::new(RwLock::new(settings::PlatformSettings::default())),
            jwt_secret,
            db_pool: None,
        }
    }

    /// Attach a Postgres pool (Wave 2). Builder-style: returns the
    /// state with `db_pool` set so `main` can do
    /// `AppState::from_env().with_db_pool(init.pool)`.
    pub fn with_db_pool(mut self, pool: Option<sqlx::PgPool>) -> Self {
        self.db_pool = pool;
        self
    }

    /// Storage mode in effect, for `/health` and logs.
    pub fn db_mode(&self) -> crate::db::DbMode {
        if self.db_pool.is_some() {
            crate::db::DbMode::Postgres
        } else {
            crate::db::DbMode::InMemory
        }
    }

    /// Create a fresh, empty state, taking the token-signing secret
    /// from the `JWT_SECRET` environment variable.
    ///
    /// When `JWT_SECRET` is not set, a fixed development fallback is
    /// used so a local instance starts without ceremony. That
    /// fallback is public knowledge: it must be overridden with a
    /// real secret in every deployed environment.
    pub fn from_env() -> Self {
        // Development-only fallback ("dev-secret-change-me"): fine
        // for a local checkout, never acceptable in production,
        // where JWT_SECRET must be set to a private random value.
        let jwt_secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| "dev-secret-change-me".to_string());
        Self::new(jwt_secret)
    }

    /// Record one entry in the audit log.
    ///
    /// This is deliberately synchronous and fire-and-forget: it is
    /// called from inside request handlers that must not fail (or
    /// block) just because they are being audited. The entry is
    /// appended only if the audit log's write lock is immediately
    /// available; if the lock is contended, the entry is skipped
    /// silently rather than stalling the request on an `await`.
    /// Callers that need a guaranteed record write the entry to
    /// [`AppState::audit_log`] themselves under an awaited lock.
    ///
    /// The recorded entry carries no request metadata: `details` is
    /// [`serde_json::Value::Null`] and both `ip_address` and
    /// `user_agent` are `None`.
    pub fn audit(
        &self,
        actor: Option<String>,
        merchant_id: Option<String>,
        action: &str,
        entity_type: &str,
        entity_id: Option<String>,
    ) {
        let entry = AuditEntry {
            id: ids::audit_id(),
            actor_user_id: actor,
            merchant_id,
            action: action.to_string(),
            entity_type: entity_type.to_string(),
            entity_id,
            ip_address: None,
            user_agent: None,
            details: serde_json::Value::Null,
            created_at: Utc::now(),
        };

        if let Ok(mut log) = self.audit_log.try_write() {
            log.push(entry);
        }
        // Contended lock: skip silently (documented above) — an
        // audit write must never fail or stall the audited action.
    }

    /// Build the gateway registry for one merchant.
    ///
    /// When the merchant has stored gateway configurations, the
    /// registry is built from them with
    /// [`GatewayRegistry::from_configs`], so the merchant's own
    /// credentials and sandbox choices apply. In every other case —
    /// the merchant has no configurations, or the configurations
    /// lock is contended and cannot be read without blocking —
    /// the registry contains every supported gateway in its
    /// unconfigured form ([`GatewayRegistry::all_unconfigured`]):
    /// gateways can be listed and inspected, but none of them
    /// reports itself as configured or can take a payment.
    ///
    /// Like [`AppState::audit`], this is synchronous and
    /// non-blocking, for the same reason: handlers call it in the
    /// middle of serving a request.
    pub fn registry_for(&self, merchant_id: &str) -> GatewayRegistry {
        match self.gateway_configs.try_read() {
            Ok(configs) => match configs.get(merchant_id) {
                Some(configs) if !configs.is_empty() => {
                    GatewayRegistry::from_configs(configs)
                }
                _ => GatewayRegistry::all_unconfigured(),
            },
            Err(_) => GatewayRegistry::all_unconfigured(),
        }
    }

    /// Seed the first administrator account, if there are no users
    /// at all yet.
    ///
    /// The seeded account has the display name `"Admin"`, the
    /// [`UserRole::Admin`] role, and a pre-verified email address;
    /// `password_hash` must already be a password hash (hashing is
    /// the caller's job — this state layer never sees plaintext
    /// credentials). When any user already exists this does
    /// nothing, so it is safe to call on every startup: only the
    /// very first boot of an empty platform creates the account.
    pub async fn seed_admin(&self, email: &str, password_hash: String) {
        let mut users = self.users.write().await;
        if !users.is_empty() {
            return;
        }

        let id = ids::user_id();
        let admin = User {
            id: id.clone(),
            name: "Admin".to_string(),
            email: email.to_string(),
            password_hash,
            role: UserRole::Admin,
            email_verified: true,
            created_at: Utc::now(),
        };
        users.insert(id, admin);
    }
}
