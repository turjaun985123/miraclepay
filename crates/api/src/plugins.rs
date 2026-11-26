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

use serde_json::{json, Value};

use miraclepay_gateways::bkash_tokenized::HttpRequestSpec;
use miraclepay_gateways::transport::{HttpTransport, ReqwestTransport};
use miraclepay_gateways::types::{
    GatewayError, InitiatedPayment, PaymentRequest, RefundRequest, RefundResult, VerifyResult,
};
use miraclepay_plugin_spi::{KvStore, PluginEngine, PluginKind, PluginManifest};
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
