pub mod auth;
pub mod error;
pub mod jobs;
pub mod models;
pub mod routes;
pub mod salesforce;
pub mod state;
pub mod ws;

pub use error::AppError;
pub use state::AppState;

use tokio::net::TcpListener;

/// Start the Axum server on the given listener.
/// Initializes AppState (DB pool, migrations, Tera), wires up the router,
/// and spawns background jobs.
///
/// Called from `src/main.rs` for standalone dev mode (`cargo watch`),
/// and from `src-tauri/src/lib.rs` via `tokio::spawn` in the Tauri bundle.
pub async fn start_server(listener: TcpListener) {
    let state: AppState = AppState::init()
        .await
        .expect("failed to initialize AppState");

    // Spawn background jobs (tokio::spawn loops — no scheduler crate)
    jobs::timers::spawn_all(
        state.db.clone(),
        state.config.db_path.clone(),
        state.config.backup_dir.clone(),
    );

    let router = routes::build_router(state).await;

    tracing::info!(
        "Inventorix listening on {}",
        listener.local_addr().unwrap()
    );

    axum::serve(listener, router)
        .await
        .expect("server error");
}
