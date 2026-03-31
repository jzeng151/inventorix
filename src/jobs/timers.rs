// Background jobs — tokio::spawn loops (no scheduler crate per CLAUDE.md)

use std::time::Duration;

/// Spawns all background jobs. Called once from `start_server`.
pub fn spawn_all(
    pool: sqlx::SqlitePool,
    db_path: String,
    backup_dir: String,
    chat_log_path: String,
) {
    // ── Refill timer check: every 5 minutes ───────────────────────────────────
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            if let Err(e) = check_expired_timers(&pool_clone).await {
                tracing::error!("Timer check failed: {e}");
            }
        }
    });

    // ── Daily jobs: backup + chat log purge ───────────────────────────────────
    tokio::spawn(async move {
        // Wait until the next midnight before the first run
        tokio::time::sleep(until_next_midnight()).await;
        loop {
            daily_backup(&db_path, &backup_dir).await;
            crate::routes::chat::purge_old_chat_logs(&chat_log_path).await;
            tokio::time::sleep(Duration::from_secs(86_400)).await;
        }
    });
}

/// Marks refill_requests as 'expired' where status='pending' AND timer_expires_at < now().
/// TODO (Lane F): implement fully.
pub async fn check_expired_timers(_pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
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
