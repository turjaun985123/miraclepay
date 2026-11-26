//! The confined plugin engine (Wave 117).
//!
//! Wraps psychoscript behind a JSON-only surface: callers pass a
//! [`serde_json::Value`] in and get one out. psychoscript types
//! never appear in this crate's public API, so the engine can be
//! swapped without breaking plugins or callers.
//!
//! Confinement, by construction:
//! - The only globals a plugin sees are the host namespaces below
//!   (`http`, `config`, `kv`, `log`, `crypto`) plus the language's
//!   own built-ins. There is no filesystem, no environment, no
//!   process access — the interpreter has none to give.
//! - `http.fetch` refuses any host not in the manifest's
//!   `allowed_hosts` before a socket is opened, and refuses
//!   private/loopback literal IPs unless explicitly allowlisted.
//! - `config.get` reads only the plugin's own configuration;
//!   `kv` is namespaced per plugin id in the backing store.
//! - Every call runs under psychoscript `Limits`: the manifest
//!   timeout (default 5 s), a 32 MiB heap cap and a 1 MiB output
//!   cap. A plugin that loops forever is interrupted by the
//!   timeout, not by killing the process.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use hmac::{Hmac, Mac};
use psychoscript::{
    Failure, HostError, HostFunction, HostResult, HostValue, Limits, Namespace, Resolved, Runtime,
};
use serde_json::{json, Value};
use sha2::Sha256;

use crate::manifest::{ManifestError, PluginManifest};

/// Per-plugin key/value backing store. The API layer implements
/// this over the persisted state; tests use [`MemoryKvStore`].
pub trait KvStore: Send + Sync {
    fn get(&self, plugin_id: &str, key: &str) -> Option<String>;
    fn set(&self, plugin_id: &str, key: &str, value: String);
}

/// In-memory [`KvStore`] (tests, and the no-database fallback).
#[derive(Default)]
pub struct MemoryKvStore(Mutex<HashMap<(String, String), String>>);

impl KvStore for MemoryKvStore {
    fn get(&self, plugin_id: &str, key: &str) -> Option<String> {
        self.0
            .lock()
            .unwrap()
            .get(&(plugin_id.to_string(), key.to_string()))
            .cloned()
    }
    fn set(&self, plugin_id: &str, key: &str, value: String) {
        self.0
            .lock()
            .unwrap()
            .insert((plugin_id.to_string(), key.to_string()), value);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("plugin io error: {0}")]
    Io(String),
    #[error("plugin runtime error ({kind}): {message}")]
    Runtime { kind: String, message: String },
    #[error("plugin does not implement operation {0}")]
    NoSuchOperation(String),
    #[error("http plugins are served by the HTTP lane, not the JS engine")]
    HttpKind,
}

/// Host context shared by every call of one plugin.
struct HostContext {
    plugin_id: String,
    allowed_hosts: Vec<String>,
    config: Mutex<Value>,
    kv: Arc<dyn KvStore>,
    logs: Mutex<Vec<String>>,
}

/// A loaded JS plugin: manifest + source + host context.
pub struct PluginEngine {
    manifest: PluginManifest,
    source: String,
    host: Arc<HostContext>,
}

fn host_err(message: impl Into<String>) -> HostValue {
    HostValue::Error(Box::new(HostError::new(message.into())))
}

fn arg_str(args: &[HostValue], idx: usize, what: &str) -> Result<String, HostValue> {
    args.get(idx)
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| host_err(format!("{what}: expected a string argument")))
}

/// Whether `host` (from a URL) is allowlisted: exact match, or a
/// subdomain of an allowlisted parent.
fn host_allowed(allowed: &[String], host: &str) -> bool {
    allowed.iter().any(|entry| {
        let entry = entry.to_ascii_lowercase();
        host == entry || host.ends_with(&format!(".{entry}"))
    })
}

/// Literal private/loopback IPs are refused unless the exact IP
/// is allowlisted — an allowlisted *domain* must not become a
/// trampoline into the LAN via a literal address.
fn literal_ip_blocked(allowed: &[String], host: &str) -> bool {
    let Ok(ip) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    if allowed.iter().any(|e| e == host) {
        return false;
    }
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

fn namespace(
    names: &'static [&'static str],
    make: impl Fn(&str, &Arc<HostContext>) -> HostFunction + Send + Sync + 'static,
    host: &Arc<HostContext>,
) -> Namespace {
    let host = host.clone();
    Namespace::new(
        move |path| match path {
            [] => Ok(names.iter().map(|n| (*n).into()).collect()),
            _ => Ok(Vec::new()),
        },
        move |path| -> Resolved {
            match path {
                [name] if name.as_str().is_some_and(|n| names.contains(&n)) => {
                    make(name.as_str().unwrap_or_default(), &host).into()
                }
                _ => Failure::missing(format!(
                    "no such host function: {}",
                    path.iter()
                        .map(|k| k.to_string())
                        .collect::<Vec<_>>()
                        .join(".")
                ))
                .into(),
            }
        },
    )
}

fn build_globals(host: &Arc<HostContext>) -> Vec<(String, HostValue)> {
    let http = namespace(
        &["fetch"],
        |_, host| {
            let host = host.clone();
            HostFunction::new(move |args, _| -> HostResult {
                let url = arg_str(&args, 0, "http.fetch")?;
                fetch(&host, &url, args.get(1))
            })
        },
        host,
    );
    let config = namespace(
        &["get"],
        |_, host| {
            let host = host.clone();
            HostFunction::new(move |args, _| -> HostResult {
                let key = arg_str(&args, 0, "config.get")?;
                let config = host.config.lock().unwrap();
                Ok(config
                    .get(&key)
                    .map(HostValue::from_json)
                    .unwrap_or(HostValue::Null))
            })
        },
        host,
    );
    let kv = namespace(
        &["get", "set"],
        |name, host| {
            let host = host.clone();
            if name == "get" {
                HostFunction::new(move |args, _| -> HostResult {
                    let key = arg_str(&args, 0, "kv.get")?;
                    Ok(host
                        .kv
                        .get(&host.plugin_id, &key)
                        .map(|v| {
                            serde_json::from_str::<Value>(&v)
                                .map(|j| HostValue::from_json(&j))
                                .unwrap_or(HostValue::Null)
                        })
                        .unwrap_or(HostValue::Null))
                })
            } else {
                HostFunction::new(move |args, _| -> HostResult {
                    let key = arg_str(&args, 0, "kv.set")?;
                    let value = args
                        .get(1)
                        .and_then(|v| v.to_json())
                        .ok_or_else(|| host_err("kv.set: value must be JSON data"))?;
                    host.kv.set(
                        &host.plugin_id,
                        &key,
                        serde_json::to_string(&value).unwrap_or_default(),
                    );
                    Ok(HostValue::Bool(true))
                })
            }
        },
        host,
    );
    let log = namespace(
        &["info", "warn", "error"],
        |name, host| {
            let host = host.clone();
            let level = name.to_string();
            HostFunction::new(move |args, _| -> HostResult {
                let message = args
                    .first()
                    .and_then(|v| v.to_json())
                    .map(|j| match &j {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();
                host.logs
                    .lock()
                    .unwrap()
                    .push(format!("[{level}] {message}"));
                Ok(HostValue::Undefined)
            })
        },
        host,
    );
    let crypto = namespace(
        &["hmac_sha256"],
        |_, _host| {
            HostFunction::new(move |args, _| -> HostResult {
                let key = arg_str(&args, 0, "crypto.hmac_sha256")?;
                let message = arg_str(&args, 1, "crypto.hmac_sha256")?;
                let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes())
                    .map_err(|_| host_err("crypto.hmac_sha256: invalid key"))?;
                mac.update(message.as_bytes());
                let hex: String = mac
                    .finalize()
                    .into_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                Ok(HostValue::from(hex))
            })
        },
        host,
    );
    vec![
        ("http".to_string(), HostValue::from(http)),
        ("config".to_string(), HostValue::from(config)),
        ("kv".to_string(), HostValue::from(kv)),
        ("log".to_string(), HostValue::from(log)),
        ("crypto".to_string(), HostValue::from(crypto)),
    ]
}

/// The one network egress a plugin has. Blocking reqwest with a
/// short timeout; the allowlist check runs before any I/O.
fn fetch(host: &HostContext, url: &str, options: Option<&HostValue>) -> HostResult {
    let parsed = url::Url::parse(url).map_err(|e| host_err(format!("http.fetch: bad URL: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(host_err("http.fetch: only http/https URLs are allowed"));
    }
    let Some(hostname) = parsed.host_str() else {
        return Err(host_err("http.fetch: URL has no host"));
    };
    let hostname = hostname.to_ascii_lowercase();
    if literal_ip_blocked(&host.allowed_hosts, &hostname) {
        return Err(host_err(format!(
            "http.fetch: private/loopback address {hostname} is not allowlisted"
        )));
    }
    if !host_allowed(&host.allowed_hosts, &hostname) {
        return Err(host_err(format!(
            "http.fetch: host {hostname} is not in this plugin's allowed_hosts"
        )));
    }
    let options_json = options.and_then(|o| o.to_json()).unwrap_or(Value::Null);
    let method = options_json
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| host_err(format!("http.fetch: client error: {e}")))?;
    let mut request = match method.as_str() {
        "POST" => client.post(parsed),
        "PUT" => client.put(parsed),
        "DELETE" => client.delete(parsed),
        _ => client.get(parsed),
    };
    if let Some(headers) = options_json.get("headers").and_then(|h| h.as_object()) {
        for (name, value) in headers {
            if let Some(value) = value.as_str() {
                request = request.header(name.as_str(), value);
            }
        }
    }
    if let Some(body) = options_json.get("body") {
        request = match body {
            Value::String(text) => request.body(text.clone()),
            Value::Null => request,
            other => request.json(other),
        };
    }
    let response = request
        .send()
        .map_err(|e| host_err(format!("http.fetch: request failed: {e}")))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .map_err(|e| host_err(format!("http.fetch: response body: {e}")))?;
    Ok(HostValue::from_json(
        &json!({"status": status, "body": body}),
    ))
}

impl PluginEngine {
    /// Load a plugin directory: parse + validate the manifest,
    /// enforce the API-version gate, read the entry source.
    pub fn load(dir: &Path, kv: Arc<dyn KvStore>, config: Value) -> Result<Self, PluginError> {
        let manifest_text = std::fs::read_to_string(dir.join("plugin.toml"))
            .map_err(|e| PluginError::Io(format!("plugin.toml: {e}")))?;
        let manifest = PluginManifest::from_toml_str(&manifest_text)?;
        manifest.check_api_version()?;
        if manifest.kind == crate::manifest::PluginKind::Http {
            return Err(PluginError::HttpKind);
        }
        let source = std::fs::read_to_string(dir.join(manifest.entry_file()))
            .map_err(|e| PluginError::Io(format!("{}: {e}", manifest.entry_file())))?;
        Ok(Self {
            host: Arc::new(HostContext {
                plugin_id: manifest.id.clone(),
                allowed_hosts: manifest.allowed_hosts.clone(),
                config: Mutex::new(config),
                kv,
                logs: Mutex::new(Vec::new()),
            }),
            manifest,
            source,
        })
    }

    /// Build an engine directly from a manifest + source (tests,
    /// and plugins delivered without a directory).
    pub fn from_source(
        manifest: PluginManifest,
        source: String,
        kv: Arc<dyn KvStore>,
        config: Value,
    ) -> Result<Self, PluginError> {
        manifest.validate()?;
        manifest.check_api_version()?;
        Ok(Self {
            host: Arc::new(HostContext {
                plugin_id: manifest.id.clone(),
                allowed_hosts: manifest.allowed_hosts.clone(),
                config: Mutex::new(config),
                kv,
                logs: Mutex::new(Vec::new()),
            }),
            manifest,
            source,
        })
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Replace the plugin's configuration (admin edits). Only
    /// this plugin's own `config.get` can see it.
    pub fn set_config(&self, config: Value) {
        *self.host.config.lock().unwrap() = config;
    }

    pub fn config(&self) -> Value {
        self.host.config.lock().unwrap().clone()
    }

    /// Log lines the plugin emitted through `log.*` (and the
    /// interpreter's console) since the last drain.
    pub fn drain_logs(&self) -> Vec<String> {
        std::mem::take(&mut *self.host.logs.lock().unwrap())
    }

    /// Invoke one plugin operation: the top-level JS function of
    /// the same name, called with `args` as its single argument.
    /// The function's return value crosses back as JSON.
    pub fn call(&self, operation: &str, args: &Value) -> Result<Value, PluginError> {
        if operation.is_empty()
            || !operation
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(PluginError::NoSuchOperation(operation.to_string()));
        }
        let args_json = serde_json::to_string(args).unwrap_or_else(|_| "null".to_string());
        let program = format!(
            "{source}\n;return (function() {{ \
             if (typeof {operation} !== \"function\") {{ throw new Error(\"__MP_NO_SUCH_OP__\"); }} \
             return {operation}({args_json}); }})();",
            source = self.source,
        );
        let host = self.host.clone();
        let runtime = Runtime::make()
            .globals(move |_| build_globals(&host))
            .limits(Limits {
                timeout_ms: Some(self.manifest.timeout_ms()),
                max_heap_bytes: Some(32 * 1024 * 1024),
                max_output_bytes: Some(1_000_000),
                ..Limits::default()
            });
        let outcome = runtime.execute(&program);
        for line in outcome.logs {
            self.host.logs.lock().unwrap().push(line.to_string());
        }
        match outcome.result {
            Ok(data) => Ok(data.to_json()),
            Err(diagnostic) => {
                let message = diagnostic.message.to_string_lossy().into_owned();
                if message.contains("__MP_NO_SUCH_OP__") {
                    Err(PluginError::NoSuchOperation(operation.to_string()))
                } else {
                    Err(PluginError::Runtime {
                        kind: format!("{:?}", diagnostic.kind),
                        message,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginKind;

    fn manifest(allowed_hosts: Vec<&str>) -> PluginManifest {
        PluginManifest {
            id: "test-plugin".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Gateway,
            provides: vec!["test_gw".to_string()],
            config_schema: None,
            allowed_hosts: allowed_hosts.iter().map(|s| s.to_string()).collect(),
            min_api_version: "1.0".to_string(),
            entry: None,
            endpoint: None,
            timeout_ms: Some(300),
        }
    }

    fn engine(source: &str, allowed: Vec<&str>) -> (PluginEngine, Arc<MemoryKvStore>) {
        let kv = Arc::new(MemoryKvStore::default());
        let engine = PluginEngine::from_source(
            manifest(allowed),
            source.to_string(),
            kv.clone(),
            json!({"api_key": "own-secret", "region": "bd"}),
        )
        .unwrap();
        (engine, kv)
    }

    #[test]
    fn calls_a_plugin_function_and_returns_json() {
        let (engine, _) = engine(
            "function add(req) { return { total: req.a + req.b }; }",
            vec![],
        );
        let out = engine.call("add", &json!({"a": 2, "b": 40})).unwrap();
        assert_eq!(out, json!({"total": 42}));
        assert!(matches!(
            engine.call("missing_op", &json!({})),
            Err(PluginError::NoSuchOperation(_))
        ));
    }

    #[test]
    fn fetch_to_a_non_allowlisted_host_is_refused_before_network() {
        let (engine, _) = engine(
            "function go(req) { return http.fetch('https://evil.example.net/x'); }",
            vec!["api.example.com"],
        );
        let err = engine.call("go", &json!({})).unwrap_err();
        assert!(err.to_string().contains("not in this plugin"), "{err}");
    }

    #[test]
    fn fetch_to_a_private_ip_is_refused() {
        let (engine, _) = engine(
            "function go(req) { return http.fetch('http://192.168.1.10/admin'); }",
            vec!["example.com"],
        );
        let err = engine.call("go", &json!({})).unwrap_err();
        assert!(err.to_string().contains("private/loopback"), "{err}");
    }

    #[test]
    fn an_infinite_loop_is_killed_by_the_timeout() {
        let (engine, _) = engine("function spin(req) { while (true) {} }", vec![]);
        let started = std::time::Instant::now();
        let err = engine.call("spin", &json!({})).unwrap_err();
        assert!(
            started.elapsed().as_secs() < 10,
            "timeout must bound the call"
        );
        assert!(
            matches!(err, PluginError::Runtime { .. }),
            "expected a runtime error, got {err}"
        );
    }

    #[test]
    fn kv_and_config_are_namespaced_per_plugin() {
        let kv = Arc::new(MemoryKvStore::default());
        let mut m1 = manifest(vec![]);
        m1.id = "plugin-one".to_string();
        let mut m2 = manifest(vec![]);
        m2.id = "plugin-two".to_string();
        let source = "function probe(req) { \
            kv.set('k', 'mine'); \
            return { mine: kv.get('k'), other_config: config.get('api_key') }; }";
        let e1 = PluginEngine::from_source(
            m1,
            source.to_string(),
            kv.clone(),
            json!({"api_key": "one"}),
        )
        .unwrap();
        let e2 = PluginEngine::from_source(
            m2,
            source.to_string(),
            kv.clone(),
            json!({"api_key": "two"}),
        )
        .unwrap();
        let out1 = e1.call("probe", &json!({})).unwrap();
        assert_eq!(out1, json!({"mine": "mine", "other_config": "one"}));
        // Plugin two cannot see plugin one's kv entry: its own
        // namespace starts empty, and its config is its own.
        let kv2 = Arc::new(MemoryKvStore::default());
        let _ = kv2;
        let e2_fresh = PluginEngine::from_source(
            manifest(vec![]),
            "function read(req) { return { v: kv.get('k') }; }".to_string(),
            kv.clone(),
            json!({}),
        )
        .unwrap();
        // e2_fresh has plugin id "test-plugin": no 'k' in its namespace.
        assert_eq!(
            e2_fresh.call("read", &json!({})).unwrap(),
            json!({"v": null})
        );
        let out2 = e2.call("probe", &json!({})).unwrap();
        assert_eq!(out2["other_config"], json!("two"));
    }

    #[test]
    fn hmac_host_function_matches_the_rust_computation() {
        let (engine, _) = engine(
            "function sign(req) { return { mac: crypto.hmac_sha256('key', 'payload') }; }",
            vec![],
        );
        let out = engine.call("sign", &json!({})).unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(b"key").unwrap();
        mac.update(b"payload");
        let expected: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(out["mac"], json!(expected));
    }
}
