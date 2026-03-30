// Lane A (Day 1): stub — modules will be filled in as each lane is built
pub mod auth;
pub mod error;
pub mod models;
pub mod routes;
pub mod salesforce;
pub mod state;
pub mod ws;
pub mod jobs;

pub use error::AppError;
pub use state::AppState;

use tokio::net::TcpListener;

/// Start the Axum server on the given listener.
/// Called from `src/main.rs` for standalone dev mode, and from
/// `src-tauri/src/lib.rs` via `tokio::spawn` for the desktop bundle.
pub async fn start_server(listener: TcpListener) {
    let router = routes::build_router().await;

    tracing::info!("Inventorix server listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, router)
        .await
        .expect("server error");
}
