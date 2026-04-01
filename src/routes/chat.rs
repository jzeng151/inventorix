use axum::{
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Form, Router,
};
use axum::extract::ws::{Message, WebSocket};
use serde::{Deserialize, Serialize};
use tera::Context;
use tokio::io::AsyncWriteExt;

use crate::{
    auth::extractor::AuthUser,
    ws::manager::WsEvent,
    AppError, AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/chat", post(send_chat))
        .route("/chat", get(chat_history))
}

// ── WebSocket upgrade ─────────────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    auth: AuthUser,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, auth))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, auth: AuthUser) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<WsEvent>(32);
    state.ws_manager.add_connection(auth.branch_id, tx);

    while let Some(event) = rx.recv().await {
        let json = match serde_json::to_string(&event) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("WS serialize error: {e}");
                continue;
            }
        };
        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }
}

// ── Global branch chat — send ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatForm {
    message: String,
}

async fn send_chat(
    State(state): State<AppState>,
    auth: AuthUser,
    Form(form): Form<ChatForm>,
) -> Result<impl IntoResponse, AppError> {
    let message = form.message.trim().to_string();
    if message.is_empty() {
        return Err(AppError::ValidationError("Message cannot be empty".into()));
    }
    if message.len() > 1_000 {
        return Err(AppError::ValidationError(
            "Message must be 1,000 characters or fewer".into(),
        ));
    }

    write_chat_log(
        &state.config.chat_log_path,
        auth.branch_id,
        auth.id,
        &auth.name,
        auth.role.as_str(),
        &message,
    )
    .await?;

    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::ChatMessage {
            sender_name: auth.name,
            role: auth.role.as_str().to_string(),
            message,
        },
    );

    Ok(StatusCode::NO_CONTENT)
}

// ── Global branch chat — history ──────────────────────────────────────────────

#[derive(Serialize)]
struct ChatEntry {
    ts: String,
    user_name: String,
    role: String,
    message: String,
}

async fn chat_history(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let messages = read_branch_chat(&state.config.chat_log_path, auth.branch_id).await;

    let mut ctx = Context::new();
    ctx.insert("messages", &messages);

    state.render("tiles/chat-messages.html", &ctx)
}

// ── Log helpers ───────────────────────────────────────────────────────────────

async fn write_chat_log(
    log_dir: &str,
    branch_id: i64,
    user_id: i64,
    user_name: &str,
    role: &str,
    message: &str,
) -> Result<(), AppError> {
    let today = chrono::Utc::now().format("%Y-%m-%d");
    let log_path = format!("{}/chat-{}.log", log_dir, today);

    if let Some(parent) = std::path::Path::new(&log_path).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Internal(format!("chat log dir: {e}")))?;
    }

    let entry = serde_json::json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "tile_id": null,
        "branch_id": branch_id,
        "user_id": user_id,
        "user_name": user_name,
        "role": role,
        "message": message,
    });

    let mut line = entry.to_string();
    line.push('\n');

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .await
        .map_err(|e| AppError::Internal(format!("chat log open: {e}")))?;

    file.write_all(line.as_bytes())
        .await
        .map_err(|e| AppError::Internal(format!("chat log write: {e}")))?;

    Ok(())
}

/// Reads all chat entries for a branch from the last 7 days of log files.
/// Returns up to 100 entries in chronological order.
async fn read_branch_chat(log_dir: &str, branch_id: i64) -> Vec<ChatEntry> {
    let mut entries: Vec<(String, ChatEntry)> = Vec::new();

    for days_ago in (0i64..7).rev() {
        let date = (chrono::Utc::now() - chrono::Duration::days(days_ago))
            .format("%Y-%m-%d");
        let log_path = format!("{}/chat-{}.log", log_dir, date);

        let Ok(content) = tokio::fs::read_to_string(&log_path).await else {
            continue;
        };

        for line in content.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v["branch_id"].as_i64() != Some(branch_id) {
                continue;
            }
            let ts = v["ts"].as_str().unwrap_or("").to_string();
            entries.push((
                ts.clone(),
                ChatEntry {
                    ts,
                    user_name: v["user_name"].as_str().unwrap_or("Unknown").to_string(),
                    role: v["role"].as_str().unwrap_or("").to_string(),
                    message: v["message"].as_str().unwrap_or("").to_string(),
                },
            ));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.len() > 100 {
        entries.drain(..entries.len() - 100);
    }
    entries.into_iter().map(|(_, e)| e).collect()
}

/// Delete chat log files older than 30 days. Called from the daily purge job.
pub async fn purge_old_chat_logs(log_dir: &str) {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(30))
        .date_naive();

    let Ok(mut dir) = tokio::fs::read_dir(log_dir).await else {
        return;
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else { continue };

        let Some(date_str) = name_str
            .strip_prefix("chat-")
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };

        let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            continue;
        };

        if date < cutoff {
            let path = entry.path();
            if let Err(e) = tokio::fs::remove_file(&path).await {
                tracing::warn!("Failed to purge chat log {:?}: {e}", path);
            } else {
                tracing::info!("Purged old chat log {:?}", path);
            }
        }
    }
}
