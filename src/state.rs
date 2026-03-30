use std::sync::Arc;
use tokio::sync::Mutex;

use crate::ws::manager::ConnectionManager;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    /// Import lock — prevents concurrent Excel imports from corrupting tile data.
    /// Returns 409 if held.
    pub import_lock: Arc<Mutex<bool>>,
    pub ws_manager: Arc<ConnectionManager>,
    pub config: AppConfig,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub db_path: String,
    pub session_secret: String,
    pub salesforce_mode: SalesforceMode,
    /// Network drive path for daily chat log files.
    /// Set via CHAT_LOG_PATH env var. See TODOS.md for IT handoff.
    pub chat_log_path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SalesforceMode {
    Mock,
    Live,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            db_path: std::env::var("DB_PATH").unwrap_or_else(|_| "./inventorix.db".to_string()),
            session_secret: std::env::var("SESSION_SECRET")
                .expect("SESSION_SECRET must be set"),
            salesforce_mode: match std::env::var("SALESFORCE_MODE")
                .unwrap_or_else(|_| "mock".to_string())
                .as_str()
            {
                "live" => SalesforceMode::Live,
                _ => SalesforceMode::Mock,
            },
            chat_log_path: std::env::var("CHAT_LOG_PATH")
                .unwrap_or_else(|_| "./chat-logs-placeholder".to_string()),
        }
    }
}
