// Background jobs — tokio::spawn loops (no scheduler crate per CLAUDE.md)

use std::time::Duration;

use crate::AppState;

/// Spawns all background jobs. Called once from `start_server`.
pub fn spawn_all(state: AppState, chat_log_path: String) {
    // ── Refill timer check: every 5 minutes ───────────────────────────────────
    let state_clone = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            if let Err(e) = check_expired_timers(&state_clone).await {
                tracing::error!("Timer check failed: {e}");
            }
        }
    });

    // ── Daily jobs: backup + chat log purge ───────────────────────────────────
    let db_path = state.config.db_path.clone();
    let backup_dir = state.config.backup_dir.clone();
    tokio::spawn(async move {
        tokio::time::sleep(until_next_midnight()).await;
        loop {
            daily_backup(&db_path, &backup_dir).await;
            crate::routes::chat::purge_old_chat_logs(&chat_log_path).await;
            tokio::time::sleep(Duration::from_secs(86_400)).await;
        }
    });
}

/// Marks pending refill_requests as 'expired' when their 48h timer has elapsed.
/// Broadcasts RefillStatusChange to the affected branch so coordinators see it live.
pub async fn check_expired_timers(state: &AppState) -> Result<(), sqlx::Error> {
    let expired = sqlx::query!(
        r#"
        SELECT rr.id, t.branch_id
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE rr.status = 'pending' AND rr.timer_expires_at < datetime('now')
        "#
    )
    .fetch_all(&state.db)
    .await?;

    for row in expired {
        sqlx::query!(
            "UPDATE refill_requests SET status = 'expired' WHERE id = ?",
            row.id
        )
        .execute(&state.db)
        .await?;

        state.ws_manager.broadcast(
            row.branch_id,
            crate::ws::manager::WsEvent::RefillStatusChange {
                refill_id: row.id,
                status: "expired".to_string(),
            },
        );

        tracing::info!("Refill request {} expired", row.id);
    }

    Ok(())
}

/// Copies inventorix.db to the backups/ directory with a datestamped filename.
/// TODO (Lane A): implement file copy.
pub async fn daily_backup(_db_path: &str, _backup_dir: &str) {}

/// Returns a Duration until the next UTC midnight.
pub fn until_next_midnight() -> Duration {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    Duration::from_secs(86_400 - (now % 86_400))
}
