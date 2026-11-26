//! Audit log domain types for MiraclePay.
//!
//! An audit entry records *who* did *what* to *which* entity, from
//! *where*, and *when*. Entries are append-only by design: this module
//! defines the record shape and the vocabulary of well-known actions,
//! while persistence and querying live in higher layers.
//!
//! The `action` field on [`AuditEntry`] is a plain `String` so callers
//! are not limited to the well-known actions in [`AuditAction`]; any
//! dotted, lower-case action name (for example `"invoice.voided"`) is
//! valid. [`AuditAction`] exists so the common, security-relevant
//! actions are spelled consistently everywhere instead of being
//! re-typed as string literals at each call site.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One immutable audit-log record.
///
/// Optional fields are `None` when the information does not apply to
/// the event: for example, a failed login attempt has no
/// `actor_user_id`, a platform-level change has no `merchant_id`, and
/// a background job has neither an `ip_address` nor a `user_agent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique identifier of this audit entry.
    pub id: String,
    /// User who performed the action, if the action is attributable
    /// to an authenticated user.
    pub actor_user_id: Option<String>,
    /// Merchant the action belongs to, if it happened inside a
    /// merchant's scope.
    pub merchant_id: Option<String>,
    /// Dotted action name, e.g. `"merchant.created"`. Prefer the
    /// values produced by [`AuditAction::as_str`] for well-known
    /// actions.
    pub action: String,
    /// Kind of entity the action acted on, e.g. `"merchant"`,
    /// `"payment"`, `"api_key"`, `"user"` or `"settings"`.
    pub entity_type: String,
    /// Identifier of the affected entity, if a single entity was
    /// affected.
    pub entity_id: Option<String>,
    /// Client IP address the action originated from, if known.
    pub ip_address: Option<String>,
    /// Client user-agent string, if the action came over HTTP.
    pub user_agent: Option<String>,
    /// Free-form, action-specific context as JSON. Callers must never
    /// put secrets (passwords, full API keys, card numbers) in here;
    /// audit logs are long-lived and widely readable inside the
    /// operator's organisation. Use [`serde_json::Value::Null`] or an
    /// empty JSON object when there is no extra context.
    pub details: serde_json::Value,
    /// When the action occurred (UTC).
    pub created_at: DateTime<Utc>,
}

impl AuditEntry {
    /// Build a new entry for a well-known [`AuditAction`].
    ///
    /// The caller supplies the identifier and timestamp so this type
    /// stays free of ID-generation and clock dependencies; all other
    /// optional context starts unset and can be filled in with the
    /// `with_*` builders or by direct field assignment.
    pub fn new(
        id: impl Into<String>,
        action: AuditAction,
        entity_type: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: id.into(),
            actor_user_id: None,
            merchant_id: None,
            action: action.as_str().to_owned(),
            entity_type: entity_type.into(),
            entity_id: None,
            ip_address: None,
            user_agent: None,
            details: serde_json::Value::Null,
            created_at,
        }
    }

    /// Set the acting user and return `self` for chaining.
    pub fn with_actor_user_id(mut self, actor_user_id: impl Into<String>) -> Self {
        self.actor_user_id = Some(actor_user_id.into());
        self
    }

    /// Set the merchant scope and return `self` for chaining.
    pub fn with_merchant_id(mut self, merchant_id: impl Into<String>) -> Self {
        self.merchant_id = Some(merchant_id.into());
        self
    }

    /// Set the affected entity id and return `self` for chaining.
    pub fn with_entity_id(mut self, entity_id: impl Into<String>) -> Self {
        self.entity_id = Some(entity_id.into());
        self
    }

    /// Set the originating IP address and return `self` for chaining.
    pub fn with_ip_address(mut self, ip_address: impl Into<String>) -> Self {
        self.ip_address = Some(ip_address.into());
        self
    }

    /// Set the originating user-agent and return `self` for chaining.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Replace the free-form details payload and return `self`.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

/// Well-known audit actions.
///
/// The string form of each variant (see [`AuditAction::as_str`]) is
/// the canonical value stored in [`AuditEntry::action`]. These
/// strings are part of the persisted data format: renaming one would
/// split the audit history for that action, so they must remain
/// stable once shipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditAction {
    /// A merchant account was created.
    #[serde(rename = "merchant.created")]
    MerchantCreated,
    /// A payment was refunded (fully or partially; the amount, if
    /// relevant, belongs in [`AuditEntry::details`]).
    #[serde(rename = "payment.refunded")]
    PaymentRefunded,
    /// Settings were changed (merchant or platform scope).
    #[serde(rename = "settings.updated")]
    SettingsUpdated,
    /// An API key was revoked.
    #[serde(rename = "api_key.revoked")]
    ApiKeyRevoked,
    /// A user logged in successfully.
    #[serde(rename = "user.login")]
    UserLogin,
}

impl AuditAction {
    /// Canonical dotted string stored in [`AuditEntry::action`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MerchantCreated => "merchant.created",
            Self::PaymentRefunded => "payment.refunded",
            Self::SettingsUpdated => "settings.updated",
            Self::ApiKeyRevoked => "api_key.revoked",
            Self::UserLogin => "user.login",
        }
    }

    /// Every well-known action, in declaration order. Useful for
    /// admin UIs that offer an action filter.
    pub fn all() -> &'static [AuditAction] {
        &[
            AuditAction::MerchantCreated,
            AuditAction::PaymentRefunded,
            AuditAction::SettingsUpdated,
            AuditAction::ApiKeyRevoked,
            AuditAction::UserLogin,
        ]
    }
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<AuditAction> for String {
    fn from(action: AuditAction) -> Self {
        action.as_str().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn merchant_created_as_str() {
        assert_eq!(AuditAction::MerchantCreated.as_str(), "merchant.created");
    }

    #[test]
    fn payment_refunded_as_str() {
        assert_eq!(AuditAction::PaymentRefunded.as_str(), "payment.refunded");
    }

    #[test]
    fn settings_updated_as_str() {
        assert_eq!(AuditAction::SettingsUpdated.as_str(), "settings.updated");
    }

    #[test]
    fn api_key_revoked_as_str() {
        assert_eq!(AuditAction::ApiKeyRevoked.as_str(), "api_key.revoked");
    }

    #[test]
    fn user_login_as_str() {
        assert_eq!(AuditAction::UserLogin.as_str(), "user.login");
    }

    #[test]
    fn display_matches_as_str() {
        for action in AuditAction::all() {
            assert_eq!(action.to_string(), action.as_str());
        }
    }

    #[test]
    fn all_contains_each_variant_once() {
        let all = AuditAction::all();
        assert_eq!(all.len(), 5);
        let mut strings: Vec<&str> = all.iter().map(|a| a.as_str()).collect();
        strings.sort_unstable();
        strings.dedup();
        assert_eq!(strings.len(), all.len(), "action strings must be unique");
    }

    #[test]
    fn action_serializes_to_its_dotted_string() {
        let json = serde_json::to_string(&AuditAction::PaymentRefunded).unwrap();
        assert_eq!(json, "\"payment.refunded\"");
        let back: AuditAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AuditAction::PaymentRefunded);
    }

    #[test]
    fn new_entry_uses_action_string_and_defaults() {
        let at = Utc.with_ymd_and_hms(2026, 11, 26, 8, 0, 0).unwrap();
        let entry = AuditEntry::new("audit_1", AuditAction::MerchantCreated, "merchant", at);
        assert_eq!(entry.id, "audit_1");
        assert_eq!(entry.action, "merchant.created");
        assert_eq!(entry.entity_type, "merchant");
        assert_eq!(entry.created_at, at);
        assert_eq!(entry.actor_user_id, None);
        assert_eq!(entry.merchant_id, None);
        assert_eq!(entry.entity_id, None);
        assert_eq!(entry.ip_address, None);
        assert_eq!(entry.user_agent, None);
        assert_eq!(entry.details, serde_json::Value::Null);
    }

    #[test]
    fn entry_builders_set_optional_fields() {
        let at = Utc.with_ymd_and_hms(2026, 11, 26, 9, 30, 0).unwrap();
        let entry = AuditEntry::new("audit_2", AuditAction::ApiKeyRevoked, "api_key", at)
            .with_actor_user_id("user_1")
            .with_merchant_id("merchant_1")
            .with_entity_id("key_1")
            .with_ip_address("203.0.113.10")
            .with_user_agent("test-agent/1.0")
            .with_details(serde_json::json!({ "reason": "rotated" }));

        assert_eq!(entry.actor_user_id.as_deref(), Some("user_1"));
        assert_eq!(entry.merchant_id.as_deref(), Some("merchant_1"));
        assert_eq!(entry.entity_id.as_deref(), Some("key_1"));
        assert_eq!(entry.ip_address.as_deref(), Some("203.0.113.10"));
        assert_eq!(entry.user_agent.as_deref(), Some("test-agent/1.0"));
        assert_eq!(entry.details["reason"], "rotated");
    }

    #[test]
    fn entry_round_trips_through_json() {
        let at = Utc.with_ymd_and_hms(2026, 11, 26, 10, 15, 0).unwrap();
        let entry = AuditEntry::new("audit_3", AuditAction::UserLogin, "user", at)
            .with_actor_user_id("user_9")
            .with_details(serde_json::json!({ "method": "password" }));

        let json = serde_json::to_string(&entry).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }
}
