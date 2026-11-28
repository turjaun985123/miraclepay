//! Plugin lifecycle + dispatch (Waves 118–120).
//!
//! At startup the API scans `PLUGINS_DIR` (default `plugins/`).
//! Each subdirectory with a `plugin.toml` becomes a
//! [`LoadedPlugin`]: JS plugins get a confined
//! [`miraclepay_plugin_spi::PluginEngine`]; `kind = "http"`
//! plugins are spoken to over the JSON protocol in
//! `docs/plugins.md`. Manifest or API-version failures are kept
//! as records with a `load_error` so the admin API can show *why*
//! a plugin did not load — they are never silently dropped and
//! never registered.
//!
//! Gateway dispatch mirrors the native registry's operations
//! (initiate / verify / refund) over JSON, so the payments and
//! refund paths can fall through from the native registry to an
//! enabled plugin that `provides` the requested gateway id. The
//! native 14 gateways are untouched.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use miraclepay_gateways::bkash_tokenized::HttpRequestSpec;
use miraclepay_gateways::transport::{HttpTransport, ReqwestTransport};
use miraclepay_gateways::types::{
    GatewayError, InitiatedPayment, PaymentRequest, RefundRequest, RefundResult, VerifyResult,
};
use miraclepay_plugin_spi::{KvStore, PluginEngine, PluginError, PluginKind, PluginManifest};
use miraclepay_sms_verify::types::ParsedSms;

use crate::state::AppState;

/// One scanned plugin directory.
pub struct LoadedPlugin {
    /// Directory name (the record key when the manifest itself
    /// failed to parse).
    pub dir_name: String,
    pub manifest: Option<PluginManifest>,
    pub engine: Option<Arc<PluginEngine>>,
    pub enabled: bool,
    pub load_error: Option<String>,
}

impl LoadedPlugin {
    pub fn id(&self) -> &str {
        self.manifest
            .as_ref()
            .map(|m| m.id.as_str())
            .unwrap_or(&self.dir_name)
    }
    pub fn kind(&self) -> Option<PluginKind> {
        self.manifest.as_ref().map(|m| m.kind)
    }
    /// Gateway ids this plugin serves (empty unless loaded).
    pub fn provides(&self) -> &[String] {
        self.manifest
            .as_ref()
            .map(|m| m.provides.as_slice())
            .unwrap_or(&[])
    }
    pub fn usable(&self) -> bool {
        self.enabled && self.load_error.is_none() && self.manifest.is_some()
    }
}

/// The persisted KV backing store: one flat map in state, keys
/// namespaced `plugin_id \u{1} key` so plugins cannot reach each
/// other's entries (the SPI only ever asks for its own id).
pub struct StateKvStore {
    map: Arc<std::sync::Mutex<HashMap<String, String>>>,
}

impl StateKvStore {
    pub fn new(map: Arc<std::sync::Mutex<HashMap<String, String>>>) -> Self {
        Self { map }
    }
    fn scoped(plugin_id: &str, key: &str) -> String {
        format!("{plugin_id}\u{1}{key}")
    }
}

impl KvStore for StateKvStore {
    fn get(&self, plugin_id: &str, key: &str) -> Option<String> {
        self.map
            .lock()
            .unwrap()
            .get(&Self::scoped(plugin_id, key))
            .cloned()
    }
    fn set(&self, plugin_id: &str, key: &str, value: String) {
        self.map
            .lock()
            .unwrap()
            .insert(Self::scoped(plugin_id, key), value);
    }
}

/// Scan a plugins directory into state. Missing directory = no
/// plugins (not an error). Called once at startup after state
/// hydration, so persisted config/enabled/KV are already in place.
pub async fn scan_plugins(state: &AppState, dir: &Path) {
    let kv: Arc<dyn KvStore> = Arc::new(StateKvStore::new(state.plugin_kv.clone()));
    let mut loaded: HashMap<String, LoadedPlugin> = HashMap::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => {
            state.plugins.write().await.clear();
            return;
        }
    };
    let mut dirs: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for path in dirs {
        let dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let record = load_one(state, &path, &dir_name, kv.clone()).await;
        loaded.insert(record.id().to_string(), record);
    }
    *state.plugins.write().await = loaded;
}

async fn load_one(
    state: &AppState,
    path: &Path,
    dir_name: &str,
    kv: Arc<dyn KvStore>,
) -> LoadedPlugin {
    let fail = |manifest: Option<PluginManifest>, error: String| LoadedPlugin {
        dir_name: dir_name.to_string(),
        manifest,
        engine: None,
        enabled: false,
        load_error: Some(error),
    };
    let text = match std::fs::read_to_string(path.join("plugin.toml")) {
        Ok(text) => text,
        Err(e) => return fail(None, format!("plugin.toml unreadable: {e}")),
    };
    let manifest = match PluginManifest::from_toml_str(&text) {
        Ok(m) => m,
        Err(e) => return fail(None, e.to_string()),
    };
    if let Err(e) = manifest.check_api_version() {
        return fail(Some(manifest), e.to_string());
    }
    let enabled = state
        .plugin_enabled
        .read()
        .await
        .get(&manifest.id)
        .copied()
        .unwrap_or(true);
    if manifest.kind == PluginKind::Http {
        return LoadedPlugin {
            dir_name: dir_name.to_string(),
            manifest: Some(manifest),
            engine: None,
            enabled,
            load_error: None,
        };
    }
    let config = state
        .plugin_configs
        .read()
        .await
        .get(&manifest.id)
        .cloned()
        .unwrap_or_else(|| json!({}));
    match PluginEngine::load(path, kv, config) {
        Ok(engine) => LoadedPlugin {
            dir_name: dir_name.to_string(),
            manifest: Some(manifest),
            engine: Some(Arc::new(engine)),
            enabled,
            load_error: None,
        },
        Err(e) => fail(Some(manifest), e.to_string()),
    }
}

/// Find the usable plugin serving `gateway_id`, if any.
async fn gateway_plugin<'a>(
    plugins: &'a HashMap<String, LoadedPlugin>,
    gateway_id: &str,
) -> Option<&'a LoadedPlugin> {
    plugins
        .values()
        .find(|p| p.usable() && p.provides().iter().any(|g| g == gateway_id))
}

/// Whether any usable plugin provides this gateway id.
pub async fn plugin_provides_gateway(state: &AppState, gateway_id: &str) -> bool {
    let plugins = state.plugins.read().await;
    gateway_plugin(&plugins, gateway_id).await.is_some()
}

/// Gateway ids served by usable plugins (the plugin lane's offered
/// set), sorted and de-duplicated. Public surfaces fold these into
/// the merchant's offered gateways so that a gateway the API
/// accepts at submit/create time can always be picked in the UI —
/// v0.0.4 Stream B FINDING-1: `submit_checkout` accepted plugin
/// gateways that `checkout_details` never offered, leaving the
/// picker unable to present a valid choice.
pub async fn plugin_gateway_ids(state: &AppState) -> Vec<String> {
    let plugins = state.plugins.read().await;
    let mut ids: Vec<String> = plugins
        .values()
        .filter(|plugin| plugin.usable())
        .flat_map(|plugin| plugin.provides().iter().cloned())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn payment_request_json(req: &PaymentRequest) -> Value {
    json!({
        "merchant_id": req.merchant_id,
        "amount_minor": req.amount.amount_minor,
        "currency": req.amount.currency.code(),
        "reference": req.reference,
        "customer_name": req.customer_name,
        "customer_email": req.customer_email,
        "customer_phone": req.customer_phone,
        "callback_url": req.callback_url,
        "metadata": req.metadata,
    })
}

/// Call one operation on the plugin serving `gateway_id` —
/// through the JS engine or the HTTP lane, same JSON in/out.
async fn call_gateway(
    state: &AppState,
    gateway_id: &str,
    operation: &str,
    args: Value,
) -> Result<Value, GatewayError> {
    enum Dispatch {
        Js(Arc<PluginEngine>),
        Http(String),
    }
    let dispatch = {
        let plugins = state.plugins.read().await;
        let plugin = gateway_plugin(&plugins, gateway_id)
            .await
            .ok_or_else(|| GatewayError::NotConfigured(format!("unknown gateway: {gateway_id}")))?;
        let manifest = plugin
            .manifest
            .as_ref()
            .expect("usable plugin has manifest");
        match manifest.kind {
            PluginKind::Http => Dispatch::Http(manifest.endpoint.clone().unwrap_or_default()),
            _ => Dispatch::Js(
                plugin
                    .engine
                    .clone()
                    .ok_or_else(|| GatewayError::NotConfigured("plugin not loaded".to_string()))?,
            ),
        }
    };
    // Both lanes are blocking work (CPU-bound interpreter,
    // blocking HTTP client): run them off the async workers.
    let operation = operation.to_string();
    let gateway_id = gateway_id.to_string();
    tokio::task::spawn_blocking(move || match dispatch {
        Dispatch::Http(endpoint) => {
            let url = format!("{}/{operation}", endpoint.trim_end_matches('/'));
            let spec = HttpRequestSpec {
                method: "POST",
                url,
                headers: vec![("Content-Type".to_string(), "application/json".to_string())],
                body: serde_json::to_string(&args).unwrap_or_default(),
            };
            let transport = ReqwestTransport::default();
            let (status, body) = transport.send(&spec)?;
            if !(200..300).contains(&status) {
                let excerpt: String = body.chars().take(200).collect();
                return Err(GatewayError::Provider(format!(
                    "plugin {gateway_id} HTTP {status}: {excerpt}"
                )));
            }
            serde_json::from_str(&body).map_err(|e| {
                GatewayError::Provider(format!("plugin {gateway_id} returned invalid JSON: {e}"))
            })
        }
        Dispatch::Js(engine) => engine
            .call(&operation, &args)
            .map_err(|e| GatewayError::Provider(e.to_string())),
    })
    .await
    .map_err(|e| GatewayError::Provider(format!("plugin task failed: {e}")))?
}

/// Plugin-lane initiate: mirrors `PaymentGateway::initiate`.
pub async fn plugin_initiate(
    state: &AppState,
    gateway_id: &str,
    req: &PaymentRequest,
) -> Result<InitiatedPayment, GatewayError> {
    let out = call_gateway(
        state,
        gateway_id,
        "create_payment",
        payment_request_json(req),
    )
    .await?;
    let provider_ref = out
        .get("provider_ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            GatewayError::Provider("plugin create_payment did not return provider_ref".to_string())
        })?
        .to_string();
    Ok(InitiatedPayment {
        provider_ref,
        redirect_url: out
            .get("redirect_url")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        instructions: out
            .get("instructions")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        expires_at: None,
    })
}

/// Plugin-lane verify: mirrors `PaymentGateway::verify`.
pub async fn plugin_verify(
    state: &AppState,
    gateway_id: &str,
    provider_ref: &str,
) -> Result<VerifyResult, GatewayError> {
    let out = call_gateway(
        state,
        gateway_id,
        "verify_payment",
        json!({"provider_ref": provider_ref}),
    )
    .await?;
    Ok(VerifyResult {
        provider_trx_id: out
            .get("provider_trx_id")
            .and_then(|v| v.as_str())
            .unwrap_or(provider_ref)
            .to_string(),
        amount_minor: out
            .get("amount_minor")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        sender: out
            .get("sender")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        verified: out
            .get("verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

/// Plugin-lane refund: mirrors `PaymentGateway::refund`.
pub async fn plugin_refund(
    state: &AppState,
    gateway_id: &str,
    req: &RefundRequest,
) -> Result<RefundResult, GatewayError> {
    let out = call_gateway(
        state,
        gateway_id,
        "refund",
        json!({
            "provider_ref": req.provider_ref,
            "amount_minor": req.amount_minor,
            "reason": req.reason,
        }),
    )
    .await?;
    Ok(RefundResult {
        provider_refund_id: out
            .get("provider_refund_id")
            .and_then(|v| v.as_str())
            .unwrap_or("plugin-refund")
            .to_string(),
        status: out
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("processing")
            .to_string(),
    })
}

/// Try the enabled SMS-parser plugins on a message the native
/// parsers could not match. Returns a [`ParsedSms`] from the
/// first plugin that reports a payment (amount + trx id).
pub async fn plugin_parse_sms(
    state: &AppState,
    sms: &miraclepay_sms_verify::types::IncomingSms,
) -> Option<ParsedSms> {
    let engines: Vec<Arc<PluginEngine>> = {
        let plugins = state.plugins.read().await;
        plugins
            .values()
            .filter(|p| p.usable() && p.kind() == Some(PluginKind::SmsParser))
            .filter_map(|p| p.engine.clone())
            .collect()
    };
    let args = json!({
        "sender": sms.sender,
        "body": sms.body,
        "received_at": sms.received_at.to_rfc3339(),
    });
    let out = tokio::task::spawn_blocking(move || {
        for engine in &engines {
            if let Ok(out) = engine.call("parse_sms", &args) {
                if out.get("matched").and_then(|v| v.as_bool()) == Some(true) {
                    return Some(out);
                }
            }
        }
        None
    })
    .await
    .ok()??;
    let amount_minor = out.get("amount_minor").and_then(|v| v.as_i64());
    let trx_id = out
        .get("trx_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if amount_minor.is_none() || trx_id.is_none() {
        return None;
    }
    let currency = out
        .get("currency")
        .and_then(|v| v.as_str())
        .and_then(miraclepay_domain::money::Currency::from_code)
        .or(Some(miraclepay_domain::money::Currency::BDT));
    Some(ParsedSms {
        gateway: None,
        amount_minor,
        currency,
        sender_number: out
            .get("sender_number")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        receiver_number: out
            .get("receiver_number")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        trx_id,
        reference: out
            .get("reference")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        balance_minor: out.get("balance_minor").and_then(|v| v.as_i64()),
        raw_sender: sms.sender.clone(),
        raw_body: sms.body.clone(),
        received_at: sms.received_at,
    })
}

// ---------------------------------------------------------------------------
// Stream D dispatch core (spec: design-audit/v004-stream-d-spec.md
// §1–§3): notification broadcast + webhook payload transformation.
// Both kinds are JS lane only (D-8): `PluginKind` is single-valued,
// so a `kind = "http"` plugin can never be a notification or
// transformer plugin and no HTTP path exists here. The existing
// gateway/SMS dispatch above is untouched.
//
// Logging: the crate's convention is `eprintln!` (spec §1.2); the
// exact line formats below are the spec's. Payloads are NEVER
// logged — ids and event types only.
// ---------------------------------------------------------------------------

/// Hard cap on how long [`apply_webhook_transformer`] may block a
/// settle response, in milliseconds (spec §3.4, D-6). The
/// effective bound is `min(manifest timeout_ms, this cap)`. Note:
/// the engine's own `Limits` timeout is the *manifest* value, so
/// when the cap fires first the detached blocking thread keeps
/// running until the engine interrupts it; the *caller* is
/// released at the cap with the original payload.
pub const TRANSFORM_TIMEOUT_CAP_MS: u64 = 2_000;

/// Per-plugin dispatch counters for the Stream D plugin kinds
/// (spec §1.1). Keyed by plugin id in
/// `AppState::plugin_dispatch_stats`; in-memory only, reset on
/// restart. A missing key means all-zero.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct DispatchStats {
    pub notify_attempted: u64,
    pub notify_succeeded: u64,
    pub notify_failed: u64,
    pub notify_timed_out: u64,
    pub notify_skipped_no_op: u64,
    pub transform_attempted: u64,
    pub transform_succeeded: u64,
    pub transform_failed: u64,
    pub transform_timed_out: u64,
    pub transform_skipped_no_op: u64,
}

/// Increment one counter for `plugin_id` under the stats write
/// lock, at the moment the outcome is known (spec §1.1).
async fn bump_dispatch_stat(
    stats: &std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, DispatchStats>>>,
    plugin_id: &str,
    bump: impl FnOnce(&mut DispatchStats),
) {
    let mut map = stats.write().await;
    bump(map.entry(plugin_id.to_string()).or_default());
}

/// A dispatch target snapshot (spec §1.3): plugin id, engine,
/// manifest timeout — taken under the plugins read lock, which is
/// dropped before any call.
type DispatchTarget = (String, Arc<PluginEngine>, u64);

/// Collect the usable JS-lane plugins of `kind`, sorted by plugin
/// id so dispatch order is deterministic (spec §1.3).
async fn collect_targets(state: &AppState, kind: PluginKind) -> Vec<DispatchTarget> {
    let plugins = state.plugins.read().await;
    let mut targets: Vec<DispatchTarget> = plugins
        .values()
        .filter(|p| p.usable() && p.kind() == Some(kind))
        .filter_map(|p| {
            let engine = p.engine.clone()?;
            let timeout_ms = p.manifest.as_ref()?.timeout_ms();
            Some((p.id().to_string(), engine, timeout_ms))
        })
        .collect();
    targets.sort_by(|a, b| a.0.cmp(&b.0));
    targets
}

/// Build the `notify` argument for a transaction outcome (spec
/// §2.1) — the ONE constructor every firing site uses, so the
/// shape is identical everywhere. Deliberately excludes customer
/// PII (`customer_email`, `customer_phone`, `customer_name`) and
/// the merchant-supplied `metadata`.
pub fn notification_arg(
    event: &str, // "payment.completed" | "payment.failed"
    event_id: &str,
    tx: &miraclepay_domain::transaction::Transaction,
    reason: Option<&str>,
) -> Value {
    json!({
        "event": event,
        "event_id": event_id,
        "merchant_id": tx.merchant_id,
        "transaction_id": tx.id,
        "gateway": tx.gateway,
        "amount_minor": tx.amount.amount_minor,
        "currency": tx.amount.currency.code(),
        "status": tx.status,
        "provider_trx_id": tx.provider_trx_id,
        "sender_number": tx.sender_number,
        "reference": tx.reference,
        "reason": reason,
        "occurred_at": chrono::Utc::now(),
    })
}

/// Fire-and-forget notification dispatch (spec §2.3). Collects the
/// usable JS-lane `notification` plugins (sorted by id) and spawns
/// ONE detached task that calls each plugin's `notify` operation
/// sequentially; returns as soon as the task is spawned. Never
/// returns an error, never awaits a plugin: a notification plugin
/// that hangs, throws, or returns garbage cannot delay the settle
/// response beyond the spawn, cannot fail the settlement, and
/// cannot prevent the webhook event from being stored (call sites
/// push the webhook event first). Return values are ignored
/// entirely (spec §2.1).
pub async fn dispatch_notification(state: &AppState, arg: Value) {
    let targets = collect_targets(state, PluginKind::Notification).await;
    if targets.is_empty() {
        return;
    }
    let stats = state.plugin_dispatch_stats.clone();
    let event = arg
        .get("event")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    let transaction_id = arg
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    tokio::spawn(async move {
        for (plugin_id, engine, timeout_ms) in targets {
            bump_dispatch_stat(&stats, &plugin_id, |s| s.notify_attempted += 1).await;
            let call_arg = arg.clone();
            let join = tokio::task::spawn_blocking(move || engine.call("notify", &call_arg));
            // The engine's own Limits interrupt the interpreter at
            // timeout_ms; the outer +2 s is a backstop against a
            // wedged blocking thread (spec §2.3).
            let bound = Duration::from_millis(timeout_ms) + Duration::from_millis(2_000);
            match tokio::time::timeout(bound, join).await {
                Ok(Ok(Ok(_))) => {
                    bump_dispatch_stat(&stats, &plugin_id, |s| s.notify_succeeded += 1).await;
                }
                Ok(Ok(Err(PluginError::NoSuchOperation(_)))) => {
                    bump_dispatch_stat(&stats, &plugin_id, |s| s.notify_skipped_no_op += 1).await;
                    eprintln!(
                        "miraclepay: plugin notify {plugin_id} does not implement notify (event {event} tx {transaction_id})"
                    );
                }
                Ok(Ok(Err(e))) => {
                    bump_dispatch_stat(&stats, &plugin_id, |s| s.notify_failed += 1).await;
                    eprintln!(
                        "miraclepay: plugin notify {plugin_id} failed for {event} tx {transaction_id}: {e}"
                    );
                }
                Ok(Err(_)) | Err(_) => {
                    bump_dispatch_stat(&stats, &plugin_id, |s| s.notify_timed_out += 1).await;
                    eprintln!(
                        "miraclepay: plugin notify {plugin_id} timed out for {event} tx {transaction_id}"
                    );
                }
            }
        }
    });
}

/// Run the merchant's opted-in webhook transformer, if any, over
/// a payload about to be stored in a `WebhookEvent` (spec §3.3).
/// Returns the payload to store — the original on no-opt-in, skip,
/// timeout, error, or invalid return. Awaited but hard-bounded
/// (§3.4): worst-case added settle latency for an opted-in
/// merchant is [`TRANSFORM_TIMEOUT_CAP_MS`]; for every
/// non-opted-in merchant it is zero (step 1 returns immediately,
/// no stats, no log). The transform can never fail a settlement,
/// never drop the webhook event, and never store a partial or
/// invalid payload.
///
/// Return contract (spec §3.1): `null` → unchanged (success); a
/// JSON object → wholesale replacement (success); any other value
/// → invalid, original kept (`transform_failed`).
pub async fn apply_webhook_transformer(
    state: &AppState,
    merchant_id: &str,
    event_type: &str,
    payload: Value,
) -> Value {
    // Step 1: the merchant's opt-in (the canonical settings field).
    let plugin_id = {
        let settings = state.merchant_settings.read().await;
        settings
            .get(merchant_id)
            .and_then(|s| s.webhook_transformer_plugin_id.clone())
    };
    let Some(plugin_id) = plugin_id.filter(|id| !id.trim().is_empty()) else {
        return payload;
    };
    // Step 2: resolve the named plugin per §1.3 (kind
    // WebhookTransformer, usable, JS engine present).
    let target = {
        let plugins = state.plugins.read().await;
        plugins
            .values()
            .find(|p| p.id() == plugin_id)
            .filter(|p| {
                p.usable() && p.kind() == Some(PluginKind::WebhookTransformer) && p.engine.is_some()
            })
            .map(|p| {
                (
                    p.engine.clone().expect("engine checked above"),
                    p.manifest.as_ref().expect("usable plugin").timeout_ms(),
                )
            })
    };
    let Some((engine, manifest_timeout_ms)) = target else {
        eprintln!(
            "miraclepay: webhook transformer {plugin_id} for merchant {merchant_id} is not usable — original payload kept"
        );
        return payload;
    };
    // Step 3: bounded call + outcome mapping (§3.1/§3.4).
    let stats = state.plugin_dispatch_stats.clone();
    bump_dispatch_stat(&stats, &plugin_id, |s| s.transform_attempted += 1).await;
    let args = json!({
        "event_type": event_type,
        "payload": payload,
        "merchant_id": merchant_id,
    });
    let join = tokio::task::spawn_blocking(move || engine.call("transform_webhook", &args));
    let effective_ms = manifest_timeout_ms.min(TRANSFORM_TIMEOUT_CAP_MS);
    match tokio::time::timeout(Duration::from_millis(effective_ms), join).await {
        Ok(Ok(Ok(Value::Null))) => {
            bump_dispatch_stat(&stats, &plugin_id, |s| s.transform_succeeded += 1).await;
            payload
        }
        Ok(Ok(Ok(out))) if out.is_object() => {
            bump_dispatch_stat(&stats, &plugin_id, |s| s.transform_succeeded += 1).await;
            out
        }
        Ok(Ok(Ok(_other))) => {
            bump_dispatch_stat(&stats, &plugin_id, |s| s.transform_failed += 1).await;
            eprintln!(
                "miraclepay: plugin transform_webhook {plugin_id} returned a non-object payload for merchant {merchant_id} event {event_type} — original payload kept"
            );
            payload
        }
        Ok(Ok(Err(PluginError::NoSuchOperation(_)))) => {
            bump_dispatch_stat(&stats, &plugin_id, |s| s.transform_skipped_no_op += 1).await;
            eprintln!(
                "miraclepay: plugin transform_webhook {plugin_id} does not implement transform_webhook for merchant {merchant_id} event {event_type} — original payload kept"
            );
            payload
        }
        Ok(Ok(Err(e))) => {
            bump_dispatch_stat(&stats, &plugin_id, |s| s.transform_failed += 1).await;
            eprintln!(
                "miraclepay: plugin transform_webhook {plugin_id} failed for merchant {merchant_id} event {event_type}: {e} — original payload kept"
            );
            payload
        }
        Ok(Err(_)) | Err(_) => {
            bump_dispatch_stat(&stats, &plugin_id, |s| s.transform_timed_out += 1).await;
            eprintln!(
                "miraclepay: plugin transform_webhook {plugin_id} timed out for merchant {merchant_id} event {event_type} — original payload kept"
            );
            payload
        }
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use crate::handlers::merchants::default_merchant_settings;
    use miraclepay_domain::money::{Currency, Money};
    use miraclepay_domain::transaction::{Transaction, TxStatus};

    fn manifest(id: &str, kind: &str) -> PluginManifest {
        PluginManifest::from_toml_str(&format!(
            "id = \"{id}\"\nname = \"T\"\nversion = \"1.0.0\"\nkind = \"{kind}\"\nmin_api_version = \"1.0\"\n"
        ))
        .expect("manifest")
    }

    async fn insert_plugin(state: &AppState, id: &str, kind: &str, source: &str) {
        let kv: Arc<dyn KvStore> = Arc::new(StateKvStore::new(state.plugin_kv.clone()));
        let m = manifest(id, kind);
        let engine = PluginEngine::from_source(m.clone(), source.to_string(), kv, json!({}))
            .expect("engine");
        state.plugins.write().await.insert(
            id.to_string(),
            LoadedPlugin {
                dir_name: id.to_string(),
                manifest: Some(m),
                engine: Some(Arc::new(engine)),
                enabled: true,
                load_error: None,
            },
        );
    }

    async fn opt_in(state: &AppState, merchant_id: &str, plugin_id: &str) {
        let mut s = default_merchant_settings(merchant_id);
        s.webhook_transformer_plugin_id = Some(plugin_id.to_string());
        state
            .merchant_settings
            .write()
            .await
            .insert(merchant_id.to_string(), s);
    }

    fn sample_tx() -> Transaction {
        Transaction {
            id: "txn_1".to_string(),
            merchant_id: "mch_1".to_string(),
            gateway: "bkash".to_string(),
            amount: Money {
                amount_minor: 50_000,
                currency: Currency::BDT,
            },
            status: TxStatus::Completed,
            customer_email: Some("secret@example.com".to_string()),
            customer_phone: Some("01700000000".to_string()),
            provider_trx_id: Some("9HX123".to_string()),
            sender_number: Some("01711111111".to_string()),
            reference: Some("order-123".to_string()),
            metadata: json!({"secret": true}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn notification_arg_has_exact_spec_shape_without_pii() {
        let arg = notification_arg("payment.completed", "evt_1", &sample_tx(), None);
        let obj = arg.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "amount_minor",
                "currency",
                "event",
                "event_id",
                "gateway",
                "merchant_id",
                "occurred_at",
                "provider_trx_id",
                "reason",
                "reference",
                "sender_number",
                "status",
                "transaction_id",
            ]
        );
        assert_eq!(arg["event"], "payment.completed");
        assert_eq!(arg["event_id"], "evt_1");
        assert_eq!(arg["amount_minor"], 50_000);
        assert_eq!(arg["currency"], "BDT");
        assert_eq!(arg["status"], "completed");
        assert_eq!(arg["reason"], Value::Null);
        assert!(arg.get("customer_email").is_none());
        assert!(arg.get("customer_phone").is_none());
        assert!(arg.get("metadata").is_none());
        // occurred_at parses as RFC 3339.
        chrono::DateTime::parse_from_rfc3339(arg["occurred_at"].as_str().expect("string"))
            .expect("rfc3339");
    }

    #[tokio::test]
    async fn transform_without_opt_in_returns_payload() {
        let state = AppState::new("test-secret".to_string());
        let payload = json!({"a": 1});
        let out =
            apply_webhook_transformer(&state, "mch_1", "payment.completed", payload.clone()).await;
        assert_eq!(out, payload);
    }

    #[tokio::test]
    async fn transform_with_unknown_plugin_returns_payload() {
        let state = AppState::new("test-secret".to_string());
        opt_in(&state, "mch_1", "nope").await;
        let payload = json!({"a": 1});
        let out =
            apply_webhook_transformer(&state, "mch_1", "payment.completed", payload.clone()).await;
        assert_eq!(out, payload);
    }

    #[tokio::test]
    async fn transform_replaces_payload_and_counts_success() {
        let state = AppState::new("test-secret".to_string());
        insert_plugin(
            &state,
            "xf",
            "webhook_transformer",
            "function transform_webhook(req) { \
               var p = req.payload; p.transformed = true; p.event = req.event_type; \
               return p; }",
        )
        .await;
        opt_in(&state, "mch_1", "xf").await;
        let out =
            apply_webhook_transformer(&state, "mch_1", "payment.completed", json!({"a": 1})).await;
        assert_eq!(
            out,
            json!({"a": 1, "transformed": true, "event": "payment.completed"})
        );
        let stats = state.plugin_dispatch_stats.read().await;
        let s = stats.get("xf").expect("stats");
        assert_eq!(s.transform_attempted, 1);
        assert_eq!(s.transform_succeeded, 1);
    }

    #[tokio::test]
    async fn transform_null_means_unchanged_and_succeeds() {
        let state = AppState::new("test-secret".to_string());
        insert_plugin(
            &state,
            "nullxf",
            "webhook_transformer",
            "function transform_webhook(req) { return null; }",
        )
        .await;
        opt_in(&state, "mch_1", "nullxf").await;
        let payload = json!({"a": 1});
        let out = apply_webhook_transformer(&state, "mch_1", "x", payload.clone()).await;
        assert_eq!(out, payload);
        let stats = state.plugin_dispatch_stats.read().await;
        assert_eq!(stats.get("nullxf").expect("stats").transform_succeeded, 1);
    }

    #[tokio::test]
    async fn transform_keeps_payload_on_error_non_object_and_no_op() {
        // Throw → failed.
        let state = AppState::new("test-secret".to_string());
        insert_plugin(
            &state,
            "boom",
            "webhook_transformer",
            "function transform_webhook(req) { throw new Error('nope'); }",
        )
        .await;
        opt_in(&state, "mch_1", "boom").await;
        let payload = json!({"a": 1});
        let out = apply_webhook_transformer(&state, "mch_1", "x", payload.clone()).await;
        assert_eq!(out, payload);
        {
            let stats = state.plugin_dispatch_stats.read().await;
            assert_eq!(stats.get("boom").expect("stats").transform_failed, 1);
        }
        // Array return → failed (non-object).
        let state2 = AppState::new("test-secret".to_string());
        insert_plugin(
            &state2,
            "arr",
            "webhook_transformer",
            "function transform_webhook(req) { return [1, 2]; }",
        )
        .await;
        opt_in(&state2, "mch_1", "arr").await;
        let out = apply_webhook_transformer(&state2, "mch_1", "x", payload.clone()).await;
        assert_eq!(out, payload);
        // Missing op → skipped_no_op.
        let state3 = AppState::new("test-secret".to_string());
        insert_plugin(
            &state3,
            "noop",
            "webhook_transformer",
            "function something_else(req) { return {}; }",
        )
        .await;
        opt_in(&state3, "mch_1", "noop").await;
        let out = apply_webhook_transformer(&state3, "mch_1", "x", payload.clone()).await;
        assert_eq!(out, payload);
        let stats = state3.plugin_dispatch_stats.read().await;
        let s = stats.get("noop").expect("stats");
        assert_eq!(s.transform_skipped_no_op, 1);
        assert_eq!(s.transform_failed, 0);
    }

    #[tokio::test]
    async fn notification_dispatch_reaches_notify_and_counts() {
        let state = AppState::new("test-secret".to_string());
        insert_plugin(
            &state,
            "notif",
            "notification",
            "function notify(event) { kv.set('seen', event.event); }",
        )
        .await;
        insert_plugin(
            &state,
            "badnotif",
            "notification",
            "function notify(event) { throw new Error('down'); }",
        )
        .await;
        insert_plugin(
            &state,
            "noopnotif",
            "notification",
            "function other(event) { return 1; }",
        )
        .await;
        let arg = notification_arg("payment.completed", "evt_1", &sample_tx(), None);
        dispatch_notification(&state, arg).await;
        // Detached task: poll the shared KV + stats briefly.
        let key = "notif\u{1}seen";
        let mut seen = None;
        for _ in 0..100 {
            seen = state.plugin_kv.lock().unwrap().get(key).cloned();
            let stats = state.plugin_dispatch_stats.read().await;
            let done = stats
                .get("badnotif")
                .map(|s| s.notify_failed == 1)
                .unwrap_or(false)
                && stats
                    .get("noopnotif")
                    .map(|s| s.notify_skipped_no_op == 1)
                    .unwrap_or(false);
            if seen.is_some() && done {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(seen.as_deref(), Some("\"payment.completed\""));
        let stats = state.plugin_dispatch_stats.read().await;
        assert_eq!(stats.get("notif").expect("stats").notify_succeeded, 1);
        assert_eq!(stats.get("badnotif").expect("stats").notify_failed, 1);
        assert_eq!(
            stats.get("noopnotif").expect("stats").notify_skipped_no_op,
            1
        );
    }
}
