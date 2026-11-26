//! MiraclePay API server binary (v0.0.2).
//! Env: PORT (default 8080), JWT_SECRET, ADMIN_EMAIL, ADMIN_PASSWORD,
//! DATABASE_URL (optional; unset = in-memory store, see `api::db`).
use miraclepay_api::{build_router, AppState};

#[tokio::main]
async fn main() {
    let db = miraclepay_api::db::init_from_env().await;
    let state = AppState::from_env().with_db_pool(db.pool);
    match miraclepay_api::persist::hydrate(&state).await {
        Ok(0) => {}
        Ok(n) => println!("[miraclepay] hydrated {n} persisted records from Postgres"),
        Err(err) => eprintln!("[miraclepay] hydration failed ({err}) — starting empty"),
    }
    miraclepay_api::persist::spawn_snapshot_task(state.clone());
    if miraclepay_api::seed::sandbox_enabled() {
        // Wave 46: sandbox mode seeds a demo dataset into an empty
        // store. The demo password is published — local evaluation
        // only, never a public deployment.
        let created = state.seed_demo().await;
        println!(
            "SANDBOX MODE: seeded {created} demo transactions; demo login {} / {} — \
             do NOT run sandbox mode on a public deployment",
            miraclepay_api::seed::DEMO_EMAIL,
            miraclepay_api::seed::DEMO_PASSWORD
        );
    }
    if let (Ok(email), Ok(password)) = (
        std::env::var("ADMIN_EMAIL"),
        std::env::var("ADMIN_PASSWORD"),
    ) {
        let hash = miraclepay_domain::password::hash_password(&password);
        state.seed_admin(&email.to_lowercase(), hash).await;
    }
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("bind");
    println!("MiraclePay API v0.0.2 listening on port {port}");
    axum::serve(listener, app).await.expect("serve");
}
