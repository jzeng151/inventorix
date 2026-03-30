use axum::{routing::get, Json, Router};
use serde_json::json;

pub fn router() -> Router {
    Router::new().route("/health", get(health_check))
}

async fn health_check() -> Json<serde_json::Value> {
    // TODO (Lane A): check DB connectivity, last timer run, active WS connections
    Json(json!({ "status": "ok" }))
}
