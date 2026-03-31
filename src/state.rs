use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use tera::Tera;

use crate::salesforce::client::{LiveClient, MockClient, SalesforceClient};
use crate::ws::manager::ConnectionManager;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    /// Import lock — prevents concurrent Excel imports from corrupting tile data.
    /// Returns 409 if held.
    pub import_lock: Arc<Mutex<bool>>,
    pub ws_manager: Arc<ConnectionManager>,
    pub salesforce: Arc<dyn SalesforceClient + Send + Sync>,
    pub tera: Arc<Tera>,
    pub config: AppConfig,
}

impl AppState {
    /// Initialize AppState: connect to SQLite (WAL + busy_timeout), run migrations,
    /// load Tera templates. Panics on unrecoverable startup errors.
    pub async fn init() -> anyhow::Result<Self> {
        let config = AppConfig::from_env();

        let opts = SqliteConnectOptions::new()
            .filename(&config.db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));

        let pool = sqlx::SqlitePool::connect_with(opts).await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        let tera = Tera::new("templates/**/*.html")?;

        let salesforce: Arc<dyn SalesforceClient + Send + Sync> =
            match config.salesforce_mode {
                SalesforceMode::Live => Arc::new(LiveClient::new(
                    std::env::var("SALESFORCE_INSTANCE_URL")
                        .unwrap_or_else(|_| "https://example.salesforce.com".to_string()),
                )),
                SalesforceMode::Mock => Arc::new(MockClient::new()),
            };

        Ok(Self {
            db: pool,
            import_lock: Arc::new(Mutex::new(false)),
            ws_manager: Arc::new(ConnectionManager::new()),
            salesforce,
            tera: Arc::new(tera),
            config,
        })
    }

    /// Render a Tera template. Use this in every handler instead of calling tera directly.
    pub fn render(&self, template: &str, ctx: &tera::Context) -> Result<axum::response::Html<String>, crate::AppError> {
        self.tera.render(template, ctx)
            .map(axum::response::Html)
            .map_err(crate::AppError::Template)
    }
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub db_path: String,
    pub session_secret: String,
    pub salesforce_mode: SalesforceMode,
    /// Network drive path for daily chat log files.
    /// Set via CHAT_LOG_PATH env var. See TODOS.md for IT handoff.
    pub chat_log_path: String,
    pub backup_dir: String,
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
            backup_dir: std::env::var("BACKUP_DIR")
                .unwrap_or_else(|_| "./backups".to_string()),
        }
    }
}
