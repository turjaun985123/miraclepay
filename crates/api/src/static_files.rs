//! Embedded frontend serving (v0.0.3.2).
//!
//! Until v0.0.3.1 the released `miraclepay-api` binary served only
//! the JSON API — the Astro frontend existed solely as a separate
//! container/site, so a user who downloaded the binary got no UI
//! at all. The frontend's own config comment always said the site
//! "can be served by any static host or by the Rust backend
//! itself"; this module makes that true.
//!
//! # Approach: rust-embed
//!
//! The built bundle (`frontend/dist`) is embedded into the binary
//! with [`rust_embed::RustEmbed`]. Chosen over a runtime `ServeDir`
//! because the release ships ONE self-contained binary: no asset
//! folder to lose, version-locked to the API it was built with,
//! and no filesystem path configuration for operators to get
//! wrong. The cost is binary size (the bundle is ~11 MB of the
//! release tarball) and a rebuild to change the UI — both
//! acceptable for a self-hosted single-binary product.
//!
//! `frontend/dist` is gitignored, so `build.rs` guarantees the
//! folder exists at compile time (a placeholder page on machines
//! that never built the frontend; CI builds the real bundle before
//! compiling Rust, so shipped binaries always carry the real UI).
//!
//! # Routing contract
//!
//! This handler is the router *fallback*: it only sees paths no
//! API route matched, so API routes always win. Inside it:
//! - `/api/...` paths (any method) and non-GET/HEAD requests keep
//!   the structured JSON 404 from `crate::versioning::api_fallback`.
//! - Asset resolution for the Astro `directory` output format:
//!   exact file → `<path>/index.html` → `/` serves `index.html`.
//! - Dynamic page shells (v0.0.3.3): the pay and checkout pages are
//!   Astro dynamic routes (`pay/[slug]`, `checkout/[id]`) built
//!   once with a placeholder segment, so the bundle contains
//!   `pay/pay/index.html` and `checkout/checkout/index.html`. A
//!   two-segment path `<seg>/<value>` with no exact/dir match
//!   falls back to the `<seg>/<seg>/index.html` shell when it
//!   exists — the page's JS reads the real slug/id from the URL.
//! - Anything else gets a small HTML 404 page.
//!
//! Cache policy: hashed `_astro/` assets are immutable for a year;
//! HTML is `no-cache` (so a new binary's pages are never masked by
//! a stale cached shell); other static files get one hour.

use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// The embedded Astro bundle. The folder path is relative to this
/// crate's manifest dir (`crates/api` → `../../frontend/dist`).
#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
struct FrontendDist;

/// Marker served by `build.rs` when no real bundle was built; the
/// integration tests use it to relax page-level assertions on
/// machines without a frontend build.
pub const PLACEHOLDER_MARKER: &str = "frontend bundle was not embedded in this build";

/// Resolve a request path to an embedded asset key, using `exists`
/// as the asset oracle. Pure and fixture-testable: production
/// passes the embed's lookup, tests pass a fixed set.
///
/// Returns `None` for paths that try to escape the bundle (`..`,
/// backslashes, NUL) and for anything not present.
pub fn resolve_with(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let clean = path.trim_start_matches('/').trim_end_matches('/');
    if clean.contains('\\') || clean.split('/').any(|seg| seg == ".." || seg == ".") {
        return None;
    }
    if clean.is_empty() {
        return exists("index.html").then(|| "index.html".to_string());
    }
    // Exact file match (favicon.svg, _astro/app.js, robots.txt, …).
    if exists(clean) {
        return Some(clean.to_string());
    }
    // Astro `directory` format: /login → login/index.html.
    let dir_index = format!("{clean}/index.html");
    if exists(&dir_index) {
        return Some(dir_index);
    }
    // Dynamic page shells: /pay/<slug> → pay/pay/index.html and
    // /checkout/<id> → checkout/checkout/index.html (the bundle's
    // placeholder-built shell for the dynamic route). Only for
    // exactly two segments, and only when that shell exists —
    // real files and real directory pages always win above.
    let segments: Vec<&str> = clean.split('/').collect();
    if segments.len() == 2 {
        let shell = format!("{0}/{0}/index.html", segments[0]);
        if exists(&shell) {
            return Some(shell);
        }
    }
    None
}

/// Resolve a request path against the real embedded bundle.
pub fn resolve_asset(path: &str) -> Option<String> {
    resolve_with(path, |key| FrontendDist::get(key).is_some())
}

/// Whether the embedded bundle contains `key` (test/smoke helper).
pub fn has_embedded(key: &str) -> bool {
    FrontendDist::get(key).is_some()
}

/// Whether the embedded bundle is the build.rs placeholder (no
/// frontend build happened on this machine).
pub fn is_placeholder_bundle() -> bool {
    match FrontendDist::get("index.html") {
        Some(file) => String::from_utf8_lossy(&file.data).contains(PLACEHOLDER_MARKER),
        None => true,
    }
}

/// Names of all embedded assets (smoke tests pick a real hashed
/// `_astro/` file from this instead of guessing a hash).
pub fn embedded_asset_names() -> Vec<String> {
    FrontendDist::iter().map(|name| name.into_owned()).collect()
}

/// Content-Type for an asset key, by extension. Unknown extensions
/// are `application/octet-stream` (with `nosniff` set by the
/// security middleware, browsers will not sniff them into HTML).
pub fn content_type_for(key: &str) -> &'static str {
    let ext = key.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "webmanifest" => "application/manifest+json",
        "map" => "application/json; charset=utf-8",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Cache-Control for an asset key: hashed build assets are
/// immutable, HTML must revalidate, everything else is short-lived.
pub fn cache_control_for(key: &str) -> &'static str {
    if key.starts_with("_astro/") {
        "public, max-age=31536000, immutable"
    } else if key.ends_with(".html") {
        "no-cache"
    } else {
        "public, max-age=3600"
    }
}

/// Router fallback: static UI for browser paths, structured JSON
/// 404 for API paths (see the module docs for the full contract).
pub async fn static_fallback(request: Request) -> Response {
    let is_read = matches!(*request.method(), Method::GET | Method::HEAD);
    let path = request.uri().path().to_string();
    if !is_read || path.starts_with("/api/") {
        return crate::versioning::api_fallback(request).await;
    }
    let head_only = *request.method() == Method::HEAD;
    match resolve_asset(&path) {
        Some(key) => serve_asset(&key, head_only),
        None => not_found_page(head_only),
    }
}

fn serve_asset(key: &str, head_only: bool) -> Response {
    let file = FrontendDist::get(key).expect("resolve_asset checked existence");
    let mut response = Response::new(if head_only {
        axum::body::Body::empty()
    } else {
        axum::body::Body::from(file.data.into_owned())
    });
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type_for(key)),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control_for(key)),
    );
    response
}

fn not_found_page(head_only: bool) -> Response {
    let body = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<title>404 — MiraclePay</title></head><body><h1>404 — page not found</h1>\
<p>This MiraclePay server could not find that page. \
<a href=\"/\">Go to the MiraclePay home page</a>.</p></body></html>";
    let mut response = (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        if head_only { "" } else { body },
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> bool {
        matches!(
            path,
            "index.html"
                | "login/index.html"
                | "dashboard/index.html"
                | "favicon.svg"
                | "_astro/app.abc123.js"
                | "_astro/site.def456.css"
                | "pay/pay/index.html"
                | "checkout/checkout/index.html"
        )
    }

    #[test]
    fn root_resolves_to_index() {
        assert_eq!(resolve_with("/", fixture).as_deref(), Some("index.html"));
        assert_eq!(resolve_with("", fixture).as_deref(), Some("index.html"));
    }

    #[test]
    fn directory_pages_resolve_to_their_index() {
        assert_eq!(
            resolve_with("/login", fixture).as_deref(),
            Some("login/index.html")
        );
        assert_eq!(
            resolve_with("/login/", fixture).as_deref(),
            Some("login/index.html")
        );
        assert_eq!(
            resolve_with("/dashboard/", fixture).as_deref(),
            Some("dashboard/index.html")
        );
    }

    #[test]
    fn exact_files_win() {
        assert_eq!(
            resolve_with("/favicon.svg", fixture).as_deref(),
            Some("favicon.svg")
        );
        assert_eq!(
            resolve_with("/_astro/app.abc123.js", fixture).as_deref(),
            Some("_astro/app.abc123.js")
        );
    }

    #[test]
    fn dynamic_routes_resolve_to_their_page_shell() {
        // /pay/<slug> and /checkout/<id> serve the placeholder-built
        // shell for their dynamic route; the page JS reads the real
        // slug/id from the browser URL.
        assert_eq!(
            resolve_with("/pay/demo-store-pay", fixture).as_deref(),
            Some("pay/pay/index.html")
        );
        assert_eq!(
            resolve_with("/pay/demo-store-pay/", fixture).as_deref(),
            Some("pay/pay/index.html")
        );
        assert_eq!(
            resolve_with("/checkout/txn_abc123", fixture).as_deref(),
            Some("checkout/checkout/index.html")
        );
        // The placeholder's own URL still resolves as a plain
        // directory page, and deeper/unknown paths do not fall back.
        assert_eq!(
            resolve_with("/pay/pay", fixture).as_deref(),
            Some("pay/pay/index.html")
        );
        assert_eq!(resolve_with("/pay/a/b", fixture), None);
        assert_eq!(resolve_with("/unknown/thing", fixture), None);
    }

    #[test]
    fn missing_and_traversal_paths_resolve_to_nothing() {
        assert_eq!(resolve_with("/nope", fixture), None);
        assert_eq!(resolve_with("/../secret", fixture), None);
        assert_eq!(resolve_with("/_astro/../../etc", fixture), None);
        assert_eq!(resolve_with("/login/../../secret", fixture), None);
    }

    #[test]
    fn content_types_cover_the_bundle_kinds() {
        assert_eq!(content_type_for("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            content_type_for("_astro/a.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(content_type_for("_astro/a.css"), "text/css; charset=utf-8");
        assert_eq!(content_type_for("favicon.svg"), "image/svg+xml");
        assert_eq!(content_type_for("font.woff2"), "font/woff2");
        assert_eq!(content_type_for("img.png"), "image/png");
        assert_eq!(content_type_for("data.bin"), "application/octet-stream");
    }

    #[test]
    fn cache_policy_immutable_assets_nocache_html() {
        assert_eq!(
            cache_control_for("_astro/app.abc123.js"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(cache_control_for("index.html"), "no-cache");
        assert_eq!(cache_control_for("favicon.svg"), "public, max-age=3600");
    }
}
