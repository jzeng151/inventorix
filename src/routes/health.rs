use axum::{extract::State, routing::get, Json, Router};
use serde_json::json;

use crate::{auth::extractor::AuthUser, AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}

async fn health_check(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Json<serde_json::Value> {
    // DB ping
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();

    // Active WebSocket connections across all branches
    let ws_connections: usize = state
        .ws_manager
        .total_connections();

    // Pending refill requests (proxy for "timer loop is running")
    let pending_refills: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM refill_requests WHERE status = 'pending'"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let status = if db_ok { "ok" } else { "degraded" };

    Json(json!({
        "status": status,
        "db": db_ok,
        "ws_connections": ws_connections,
        "pending_refills": pending_refills,
    }))
}
