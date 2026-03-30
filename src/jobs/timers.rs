// Lane A / Lane F: background jobs — tokio::spawn loops, no scheduler crate

use std::time::Duration;

/// Spawns both background jobs. Call once from `start_server`.
pub fn spawn_all(_pool: sqlx::SqlitePool, _db_path: String, _backup_dir: String) {
    // TODO (Lane A/F): spawn refill timer loop + daily backup loop
}

/// Marks refill_requests as 'expired' where status = 'pending' AND timer_expires_at < now().
/// Runs every 5 minutes.
pub async fn check_expired_timers(_pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    // TODO (Lane F): implement with sqlx::query!
    Ok(())
}

/// Copies inventorix.db to the backups/ directory.
/// Runs once per day at midnight.
pub async fn daily_backup(_db_path: &str, _backup_dir: &str) {
    // TODO (Lane A): implement file copy
}

/// Returns a Duration until the next midnight (UTC).
pub fn until_next_midnight() -> Duration {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let seconds_since_midnight = now % 86_400;
    let seconds_until_midnight = 86_400 - seconds_since_midnight;
    Duration::from_secs(seconds_until_midnight)
}
