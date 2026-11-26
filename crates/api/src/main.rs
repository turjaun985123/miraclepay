//! MiraclePay API server binary (v0.0.1).
//! Env: PORT (default 8080), JWT_SECRET, ADMIN_EMAIL, ADMIN_PASSWORD.
use miraclepay_api::{build_router, AppState};

#[tokio::main]
async fn main() {
    let state = AppState::from_env();
    if let (Ok(email), Ok(password)) = (
        std::env::var("ADMIN_EMAIL"),
        std::env::var("ADMIN_PASSWORD"),
    ) {
        let hash = miraclepay_domain::api_key::hash_key(&format!("{}::{}", email.to_lowercase(), password));
        state.seed_admin(&email.to_lowercase(), hash).await;
    }
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.expect("bind");
    println!("MiraclePay API v0.0.1 listening on port {port}");
    axum::serve(listener, app).await.expect("serve");
}
