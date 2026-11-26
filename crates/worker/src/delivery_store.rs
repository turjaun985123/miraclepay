//! Persistent webhook-delivery store (Wave 6).
//!
//! Until this wave, deliveries lived only in the API's in-memory
//! vector: a restart lost every pending retry. [`FileDeliveryStore`]
//! persists deliveries as JSON on disk (atomic tmp-file + rename on
//! every mutation), separates exhausted deliveries into a
//! dead-letter list, and answers "what is due now" for the worker's
//! scheduler loop.
//!
//! The store is deliberately storage-shaped, not HTTP-shaped: the
//! delivery engine ([`crate::webhook_delivery`]) performs attempts
//! and hands updated [`WebhookDelivery`] records back via
//! [`FileDeliveryStore::record`], which routes terminal
//! `DeadLetter` records to the dead-letter list automatically.
//!
//! On the Postgres path (Wave 2) the same record shape maps to the
//! `webhook_deliveries` table from migration 0002; this file store
//! is the no-database persistence option and the test fixture.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use miraclepay_domain::webhook::{DeliveryStatus, WebhookDelivery};
use serde::{Deserialize, Serialize};

/// On-disk shape: active deliveries plus the dead-letter list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedState {
    deliveries: Vec<WebhookDelivery>,
    dead_letter: Vec<WebhookDelivery>,
}

/// A JSON-file-backed store for webhook deliveries.
#[derive(Debug, Clone)]
pub struct FileDeliveryStore {
    path: PathBuf,
    state: PersistedState,
}

impl FileDeliveryStore {
    /// Open (or create) the store at `path`. A missing file starts
    /// empty; a corrupt file is reported as an error rather than
    /// silently discarding delivery history.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let state = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?
        } else {
            PersistedState::default()
        };
        Ok(Self { path, state })
    }

    /// Insert or replace a delivery (matched by `id`) and persist.
    ///
    /// A delivery whose status is [`DeliveryStatus::DeadLetter`] is
    /// moved to the dead-letter list; any other status stays active.
    pub fn record(&mut self, delivery: WebhookDelivery) -> Result<(), String> {
        self.state.deliveries.retain(|d| d.id != delivery.id);
        self.state.dead_letter.retain(|d| d.id != delivery.id);
        if delivery.status == DeliveryStatus::DeadLetter {
            self.state.dead_letter.push(delivery);
        } else {
            self.state.deliveries.push(delivery);
        }
        self.persist()
    }

    /// Deliveries due for an attempt at `now`: retryable, and either
    /// never attempted (`next_retry_at` is `None` while pending) or
    /// scheduled at or before `now`.
    pub fn due(&self, now: DateTime<Utc>) -> Vec<WebhookDelivery> {
        self.state
            .deliveries
            .iter()
            .filter(|d| d.is_retryable())
            .filter(|d| match d.next_retry_at {
                None => true,
                Some(at) => at <= now,
            })
            .cloned()
            .collect()
    }

    /// All active (non-dead-letter) deliveries.
    pub fn deliveries(&self) -> &[WebhookDelivery] {
        &self.state.deliveries
    }

    /// Deliveries that exhausted all attempts, awaiting manual
    /// inspection or redelivery.
    pub fn dead_letter(&self) -> &[WebhookDelivery] {
        &self.state.dead_letter
    }

    /// Requeue a dead-lettered delivery for a fresh attempt series:
    /// status back to `Pending`, attempts reset, retry unscheduled.
    /// Returns `false` when no such delivery exists.
    pub fn requeue_dead_letter(&mut self, id: &str) -> Result<bool, String> {
        let Some(pos) = self.state.dead_letter.iter().position(|d| d.id == id) else {
            return Ok(false);
        };
        let mut delivery = self.state.dead_letter.remove(pos);
        delivery.status = DeliveryStatus::Pending;
        delivery.attempts = 0;
        delivery.next_retry_at = None;
        self.state.deliveries.push(delivery);
        self.persist()?;
        Ok(true)
    }

    fn persist(&self) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(&self.state)
            .map_err(|e| format!("serialise deliveries: {e}"))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, raw).map_err(|e| format!("write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| format!("rename to {}: {e}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn delivery(id: &str, status: DeliveryStatus, attempts: u32) -> WebhookDelivery {
        WebhookDelivery {
            id: id.to_string(),
            event_id: "evt_1".to_string(),
            url: "https://example.com/hook".to_string(),
            status,
            attempts,
            last_status_code: None,
            next_retry_at: None,
        }
    }

    fn temp_store() -> (std::path::PathBuf, FileDeliveryStore) {
        // Unique per call: tests run in parallel threads and must not
        // share a file (the store persists via tmp-file + rename).
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "miraclepay-deliveries-test-{}-{seq}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = FileDeliveryStore::open(&path).unwrap();
        (path, store)
    }

    #[test]
    fn records_survive_reopen() {
        let (path, mut store) = temp_store();
        store
            .record(delivery("d1", DeliveryStatus::Pending, 0))
            .unwrap();
        drop(store);
        let reopened = FileDeliveryStore::open(&path).unwrap();
        assert_eq!(reopened.deliveries().len(), 1);
        assert_eq!(reopened.deliveries()[0].id, "d1");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dead_letter_is_separated_automatically() {
        let (_path, mut store) = temp_store();
        store
            .record(delivery("d1", DeliveryStatus::Failed, 7))
            .unwrap();
        store
            .record(delivery("d1", DeliveryStatus::DeadLetter, 8))
            .unwrap();
        assert!(store.deliveries().is_empty());
        assert_eq!(store.dead_letter().len(), 1);
    }

    #[test]
    fn due_respects_retry_schedule_and_terminal_states() {
        let (_path, mut store) = temp_store();
        let now = Utc::now();
        let mut future = delivery("future", DeliveryStatus::Failed, 1);
        future.next_retry_at = Some(now + Duration::hours(1));
        store.record(future).unwrap();
        store
            .record(delivery("ready", DeliveryStatus::Pending, 0))
            .unwrap();
        store
            .record(delivery("done", DeliveryStatus::Delivered, 1))
            .unwrap();
        let due = store.due(now);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "ready");
        // An hour later the scheduled retry becomes due too.
        assert_eq!(store.due(now + Duration::hours(2)).len(), 2);
    }

    #[test]
    fn requeue_dead_letter_resets_attempts() {
        let (_path, mut store) = temp_store();
        store
            .record(delivery("d9", DeliveryStatus::DeadLetter, 8))
            .unwrap();
        assert!(store.requeue_dead_letter("d9").unwrap());
        assert!(store.dead_letter().is_empty());
        let d = &store.deliveries()[0];
        assert_eq!(d.status, DeliveryStatus::Pending);
        assert_eq!(d.attempts, 0);
        assert!(!store.requeue_dead_letter("missing").unwrap());
    }

    #[test]
    fn corrupt_file_is_an_error_not_silent_data_loss() {
        let path = std::env::temp_dir().join(format!(
            "miraclepay-deliveries-corrupt-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, "{not json").unwrap();
        assert!(FileDeliveryStore::open(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
