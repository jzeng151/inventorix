use axum::Router;

// Lane A–F handlers added here as each lane is built
pub mod admin;
pub mod chat;
pub mod export;
pub mod health;
pub mod import;
pub mod refill;
pub mod tiles;

/// Assembles the full Axum router.
/// Called from `inventorix_server::start_server`.
pub async fn build_router() -> Router {
    Router::new()
        .merge(health::router())
    // TODO (Lane A): add session layer, auth routes, and all feature routers
}
