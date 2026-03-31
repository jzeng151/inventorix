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
/// Skips silently if db_path is ":memory:" (test environment).
pub async fn daily_backup(db_path: &str, backup_dir: &str) {
    if db_path == ":memory:" {
        return;
    }

    let today = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // secs → days since epoch → rough YYYY-MM-DD via chrono-free math
        // Use a simple format: days since epoch won't give us a calendar date without chrono.
        // We'll use the file timestamp in epoch days as a unique suffix instead.
        let days = secs / 86_400;
        // Convert to approximate YYYY-MM-DD
        // Days since 1970-01-01; use chrono is not a dep — compute manually
        // Simpler: use std::process to run `date` is not allowed. Use a small algorithm.
        epoch_days_to_date(days)
    };

    if let Err(e) = tokio::fs::create_dir_all(backup_dir).await {
        tracing::error!("Backup: could not create backup_dir {backup_dir}: {e}");
        return;
    }

    let dest = format!("{backup_dir}/inventorix-{today}.db");
    match tokio::fs::copy(db_path, &dest).await {
        Ok(bytes) => tracing::info!("Daily backup: {bytes} bytes → {dest}"),
        Err(e) => tracing::error!("Daily backup failed: {e}"),
    }
}

/// Converts days since Unix epoch (1970-01-01) to a YYYY-MM-DD string.
fn epoch_days_to_date(days: u64) -> String {
    // Gregorian calendar algorithm (no external crate)
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Returns a Duration until the next UTC midnight.
pub fn until_next_midnight() -> Duration {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    Duration::from_secs(86_400 - (now % 86_400))
}
