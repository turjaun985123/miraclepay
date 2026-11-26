//! Build script for miraclepay-api (v0.0.3.2 embedded UI).
//!
//! The api binary embeds the built Astro frontend (`frontend/dist`)
//! with rust-embed — see `src/static_files.rs`. rust-embed requires
//! the folder to exist at compile time, but `frontend/dist` is a
//! gitignored build artifact: on a fresh clone (or any machine that
//! has not built the frontend) it does not exist, and a missing
//! folder would make `cargo test` fail for every contributor.
//!
//! So: if `frontend/dist/index.html` is missing, create the folder
//! with a small placeholder page. The placeholder keeps the crate
//! compiling and the test suite green everywhere; it is replaced by
//! the real bundle the moment anyone runs `npm run build` in
//! `frontend/` (Astro empties its output dir on build). CI builds
//! the real frontend BEFORE compiling Rust, so shipped binaries
//! always embed the real UI — the placeholder can only ever appear
//! in a local dev binary built without a frontend build, and the
//! placeholder page says exactly that.

use std::fs;
use std::path::PathBuf;

const PLACEHOLDER_INDEX: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>MiraclePay</title>
</head>
<body>
<h1>MiraclePay</h1>
<p>The frontend bundle was not embedded in this build
(placeholder page). Build it with
<code>npm install &amp;&amp; npm run build</code> in
<code>frontend/</code>, then rebuild this binary.
The API itself is fully available under <code>/api/v1</code>.</p>
</body>
</html>
"#;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let dist = manifest_dir.join("../../frontend/dist");
    // Rebuild when the real bundle changes so release binaries pick
    // up fresh assets without a manual `cargo clean`.
    println!("cargo:rerun-if-changed={}", dist.display());
    let index = dist.join("index.html");
    if !index.exists() {
        fs::create_dir_all(&dist).expect("create frontend/dist placeholder dir");
        fs::write(&index, PLACEHOLDER_INDEX).expect("write placeholder index.html");
    }
}
