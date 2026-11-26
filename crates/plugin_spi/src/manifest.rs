//! Plugin manifest (`plugin.toml`) — the public contract between
//! MiraclePay and a plugin. No engine types appear here.

use serde::Deserialize;

/// The plugin API version this build of MiraclePay implements.
/// A plugin whose `min_api_version` is incompatible (different
/// major, or a minor newer than this) refuses to load.
pub const PLUGIN_API_VERSION: &str = "1.0";

/// What a plugin provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// A payment gateway (JS lane).
    Gateway,
    /// An SMS format parser.
    SmsParser,
    /// A notification channel.
    Notification,
    /// A webhook payload transformer.
    WebhookTransformer,
    /// An out-of-process gateway spoken to over HTTP (JSON
    /// protocol, see docs/plugins.md) — no JS is loaded.
    Http,
}

impl PluginKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginKind::Gateway => "gateway",
            PluginKind::SmsParser => "sms_parser",
            PluginKind::Notification => "notification",
            PluginKind::WebhookTransformer => "webhook_transformer",
            PluginKind::Http => "http",
        }
    }
}

/// A parsed `plugin.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin id: lowercase letters, digits, dashes.
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    /// Gateway ids this plugin provides (gateway/http kinds).
    #[serde(default)]
    pub provides: Vec<String>,
    /// JSON Schema for the plugin's configuration, as a TOML
    /// table; converted to JSON for the admin form generator.
    #[serde(default)]
    pub config_schema: Option<toml::Value>,
    /// Hosts `http.fetch` may reach. Everything else is refused.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Minimum plugin API version the plugin needs.
    pub min_api_version: String,
    /// Entry file inside the plugin directory (default `main.js`).
    #[serde(default)]
    pub entry: Option<String>,
    /// Base URL for `kind = "http"` plugins.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Per-call timeout override in milliseconds (default 5000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest does not parse: {0}")]
    Parse(String),
    #[error("manifest is invalid: {0}")]
    Invalid(String),
    #[error("plugin requires API {required}, this MiraclePay provides {provided}")]
    IncompatibleApi { required: String, provided: String },
}

/// Parse a (major, minor) version; anything else is None.
fn parse_version(v: &str) -> Option<(u64, u64)> {
    let mut parts = v.trim().split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

/// Whether a plugin requiring `min_api_version` can run on this
/// build: same major, and the plugin's minor must not exceed the
/// provided minor.
pub fn api_compatible(min_api_version: &str) -> bool {
    match (
        parse_version(min_api_version),
        parse_version(PLUGIN_API_VERSION),
    ) {
        (Some((req_major, req_minor)), Some((prov_major, prov_minor))) => {
            req_major == prov_major && req_minor <= prov_minor
        }
        _ => false,
    }
}

impl PluginManifest {
    pub fn from_toml_str(text: &str) -> Result<Self, ManifestError> {
        let manifest: PluginManifest =
            toml::from_str(text).map_err(|e| ManifestError::Parse(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn entry_file(&self) -> &str {
        self.entry.as_deref().unwrap_or("main.js")
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms.unwrap_or(5_000)
    }

    /// Structural validation (everything except API-version
    /// compatibility, which the loader checks so it can report the
    /// reason separately).
    pub fn validate(&self) -> Result<(), ManifestError> {
        let invalid = |msg: &str| ManifestError::Invalid(msg.to_string());
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(invalid("id must be lowercase letters, digits and dashes"));
        }
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            return Err(invalid("name and version are required"));
        }
        if parse_version(&self.min_api_version).is_none() {
            return Err(invalid("min_api_version must look like \"1.0\""));
        }
        if matches!(self.kind, PluginKind::Gateway | PluginKind::Http)
            && self.provides.is_empty()
        {
            return Err(invalid(
                "gateway plugins must list at least one id in provides",
            ));
        }
        if self.kind == PluginKind::Http {
            match &self.endpoint {
                Some(url) if url.starts_with("https://") || url.starts_with("http://localhost") => {
                }
                _ => {
                    return Err(invalid(
                        "http plugins require an https endpoint (http allowed only for localhost)",
                    ))
                }
            }
        }
        if let Some(entry) = &self.entry {
            if entry.contains('/') || entry.contains('\\') || entry.contains("..") {
                return Err(invalid("entry must be a plain file name"));
            }
        }
        if self.timeout_ms.is_some_and(|t| t == 0 || t > 60_000) {
            return Err(invalid("timeout_ms must be within 1..=60000"));
        }
        Ok(())
    }

    /// Loader gate: refuse incompatible plugins with the reason.
    pub fn check_api_version(&self) -> Result<(), ManifestError> {
        if api_compatible(&self.min_api_version) {
            Ok(())
        } else {
            Err(ManifestError::IncompatibleApi {
                required: self.min_api_version.clone(),
                provided: PLUGIN_API_VERSION.to_string(),
            })
        }
    }

    /// The config schema as JSON (for the admin form generator).
    pub fn config_schema_json(&self) -> serde_json::Value {
        match &self.config_schema {
            Some(value) => serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
            None => serde_json::Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
id = "manual-bank-transfer"
name = "Manual Bank Transfer"
version = "1.0.0"
kind = "gateway"
provides = ["manual_bank_transfer"]
min_api_version = "1.0"
"#;

    #[test]
    fn parses_and_validates() {
        let m = PluginManifest::from_toml_str(GOOD).unwrap();
        assert_eq!(m.id, "manual-bank-transfer");
        assert_eq!(m.kind, PluginKind::Gateway);
        assert_eq!(m.entry_file(), "main.js");
        assert_eq!(m.timeout_ms(), 5_000);
        m.check_api_version().unwrap();
    }

    #[test]
    fn bad_id_and_missing_provides_are_rejected() {
        let bad = GOOD.replace("manual-bank-transfer", "Bad Id!");
        assert!(PluginManifest::from_toml_str(&bad).is_err());
        let no_provides = GOOD.replace("provides = [\"manual_bank_transfer\"]", "provides = []");
        assert!(PluginManifest::from_toml_str(&no_provides).is_err());
    }

    #[test]
    fn incompatible_api_versions_refuse() {
        assert!(api_compatible("1.0"));
        assert!(!api_compatible("2.0"));
        assert!(!api_compatible("1.1"));
        assert!(!api_compatible("banana"));
        let future = GOOD.replace("min_api_version = \"1.0\"", "min_api_version = \"2.0\"");
        let m = PluginManifest::from_toml_str(&future).unwrap();
        assert!(matches!(
            m.check_api_version(),
            Err(ManifestError::IncompatibleApi { .. })
        ));
    }

    #[test]
    fn http_kind_requires_an_endpoint() {
        let http = r#"
id = "remote-gw"
name = "Remote"
version = "0.1.0"
kind = "http"
provides = ["remote_gw"]
min_api_version = "1.0"
"#;
        assert!(PluginManifest::from_toml_str(http).is_err());
        let with = format!("{http}\nendpoint = \"https://gw.example.com/api\"\n");
        assert!(PluginManifest::from_toml_str(&with).is_ok());
    }
}
