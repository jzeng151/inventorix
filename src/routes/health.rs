use axum::{routing::get, Json, Router};
use serde_json::json;

/// Generic over S so this router merges into any typed Router<S>.
/// health_check doesn't use app state, so it's compatible with any state.
pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/health", get(health_check))
}

async fn health_check() -> Json<serde_json::Value> {
    // TODO (Lane A): check DB connectivity, last timer run, active WS connections
    Json(json!({ "status": "ok" }))
}
