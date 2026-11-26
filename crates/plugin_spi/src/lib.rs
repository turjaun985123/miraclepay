//! MiraclePay Plugin SPI (Waves 116–121).
//!
//! The public contract between MiraclePay and its plugins:
//! a `plugin.toml` manifest ([`manifest`]) plus a confined
//! JavaScript engine ([`engine`]) whose only contact with the
//! outside world is the host-function set documented in
//! `docs/plugins.md`.
//!
//! The engine is psychoscript (MIT, pure-Rust confined JS
//! interpreter), pinned to an exact git revision in this crate's
//! `Cargo.toml`. Its types are confined to [`engine`]'s private
//! internals: everything crossing this crate's boundary is
//! `serde_json` data and the types below, so the engine can be
//! replaced without changing a single plugin or caller.

pub mod engine;
pub mod manifest;

pub use engine::{KvStore, MemoryKvStore, PluginEngine, PluginError};
pub use manifest::{api_compatible, ManifestError, PluginKind, PluginManifest, PLUGIN_API_VERSION};
