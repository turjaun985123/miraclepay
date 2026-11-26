//! MiraclePay API server binary (v0.0.2).
//! Env: PORT (default 8080), JWT_SECRET, ADMIN_EMAIL, ADMIN_PASSWORD,
//! DATABASE_URL (optional; unset = in-memory store, see `api::db`).
use miraclepay_api::{build_router, AppState};

#[tokio::main]
async fn main() {
    // Wave 93: validate configuration before anything starts.
    let production = miraclepay_api::config::is_production();
    let problems =
        miraclepay_api::config::validate_env(|name| std::env::var(name).ok(), production);
    for problem in &problems {
        eprintln!("[miraclepay] config: {problem}");
    }
    if production && !problems.is_empty() {
        eprintln!("[miraclepay] refusing to start in production with invalid configuration");
        std::process::exit(2);
    }
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
    // Waves 118-120: load plugins (JS lane via psychoscript +
    // HTTP lane) after hydration so persisted config/KV/enabled
    // flags are in place. A missing directory simply means no
    // plugins; a broken plugin is recorded with its load error.
    let plugins_dir = std::env::var("PLUGINS_DIR").unwrap_or_else(|_| "plugins".to_string());
    miraclepay_api::plugins::scan_plugins(&state, std::path::Path::new(&plugins_dir)).await;
    {
        let plugins = state.plugins.read().await;
        for plugin in plugins.values() {
            match &plugin.load_error {
                Some(error) => {
                    println!(
                        "[miraclepay] plugin {} failed to load: {error}",
                        plugin.id()
                    )
                }
                None => println!(
                    "[miraclepay] plugin {} loaded (enabled: {})",
                    plugin.id(),
                    plugin.enabled
                ),
            }
        }
    }
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let app = build_router(state.clone());
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("bind");
    println!("MiraclePay API v0.0.2 listening on port {port}");
    // Wave 92: graceful shutdown on Ctrl-C / SIGTERM, then a final
    // persistence snapshot so the snapshot window closes to zero
    // on a clean stop.
    let shutdown = async {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm handler");
        #[cfg(unix)]
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
        #[cfg(not(unix))]
        let _ = ctrl_c.await;
        println!("[miraclepay] shutdown signal received — draining");
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .expect("serve");
    match miraclepay_api::persist::snapshot(&state).await {
        Ok(0) => {}
        Ok(n) => println!("[miraclepay] final persistence snapshot: {n} records"),
        Err(err) => eprintln!("[miraclepay] final persistence snapshot failed: {err}"),
    }
}
