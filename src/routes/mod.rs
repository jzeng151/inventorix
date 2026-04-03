use axum::Router;
use tower_http::services::ServeDir;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions::cookie::time::Duration;
use tower_sessions::cookie::SameSite;
use tower_sessions_sqlx_store::SqliteStore;

use crate::AppState;

pub mod admin;
pub mod analytics;
pub mod chat;
pub mod export;
pub mod health;
pub mod history;
pub mod import;
pub mod refill;
pub mod tiles;

/// Assembles the full Axum router with session middleware and static file serving.
pub async fn build_router(state: AppState) -> Router {
    // Session store backed by the same SQLite DB.
    // Creates a `tower_sessions` table if it doesn't exist.
    let session_store = SqliteStore::new(state.db.clone());
    session_store
        .migrate()
        .await
        .expect("failed to migrate session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)                         // HTTP on LAN — no HTTPS required
        .with_http_only(true)                       // prevent JS from reading the cookie
        .with_same_site(SameSite::Strict)           // CSRF mitigation
        .with_expiry(Expiry::OnInactivity(Duration::hours(8)));

    Router::new()
        // Auth
        .merge(crate::auth::routes::router())
        // Feature routes
        .merge(health::router())
        .merge(tiles::router())
        .merge(chat::router())
        .merge(analytics::router())
        .merge(history::router())
        .merge(import::router())
        .merge(export::router())
        .merge(refill::router())
        .merge(admin::router())
        // Static assets (CSS, HTMX)
        .nest_service("/static", ServeDir::new("static"))
        .layer(session_layer)
        .with_state(state)
}
