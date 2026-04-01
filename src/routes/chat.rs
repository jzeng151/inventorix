use axum::{
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Form, Json, Router,
};
use axum::extract::ws::{Message, WebSocket};
use serde::{Deserialize, Serialize};
use tera::Context;
use tokio::io::AsyncWriteExt;

use crate::{
    auth::extractor::{AuthUser, Role},
    ws::manager::WsEvent,
    AppError, AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/chat", post(send_chat))
        .route("/chat", get(chat_history))
        .route("/chat/handled", post(mark_handled))
        .route("/chat/handled", get(list_handled))
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
    time_str: String,      // e.g. "2:30 PM"
    date_str: String,      // e.g. "Mar 31, 2026"
    show_date_sep: bool,   // true when this is the first message of a new calendar day
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

// ── POST /chat/handled — record a give-out so chat history shows strikethrough ─

#[derive(Deserialize)]
struct HandledForm {
    item_number: String,
}

async fn mark_handled(
    State(state): State<AppState>,
    auth: AuthUser,
    Form(form): Form<HandledForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }
    if form.item_number.is_empty() || form.item_number.len() > 100 {
        return Err(AppError::ValidationError("Invalid item number".into()));
    }

    sqlx::query!(
        "INSERT INTO chat_give_outs (branch_id, item_number, handled_by_name) VALUES (?, ?, ?)",
        auth.branch_id, form.item_number, auth.name
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ── GET /chat/handled — return handled items for this branch ──────────────────

#[derive(Serialize)]
struct HandledItem {
    item_number: String,
    handled_by_name: String,
}

async fn list_handled(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query!(
        r#"
        SELECT item_number, handled_by_name
        FROM chat_give_outs
        WHERE branch_id = ?
        ORDER BY handled_at DESC
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    let items: Vec<HandledItem> = rows
        .into_iter()
        .map(|r| HandledItem {
            item_number: r.item_number,
            handled_by_name: r.handled_by_name,
        })
        .collect();

    Ok(Json(items))
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
    let mut entries: Vec<(String, String, String, String)> = Vec::new();

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
                v["user_name"].as_str().unwrap_or("Unknown").to_string(),
                v["role"].as_str().unwrap_or("").to_string(),
                v["message"].as_str().unwrap_or("").to_string(),
            ));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.len() > 100 {
        entries.drain(..entries.len() - 100);
    }

    let mut prev_date = String::new();
    entries
        .into_iter()
        .map(|(ts, user_name, role, message)| {
            let (time_str, date_str) = fmt_chat_ts(&ts);
            let show_date_sep = date_str != prev_date;
            if show_date_sep {
                prev_date = date_str.clone();
            }
            ChatEntry { ts, user_name, role, message, time_str, date_str, show_date_sep }
        })
        .collect()
}

/// Parse an RFC 3339 timestamp and return (time_str, date_str).
/// time_str: "2:30 PM"   date_str: "Mar 31, 2026"
fn fmt_chat_ts(ts: &str) -> (String, String) {
    use chrono::{Datelike, Timelike};
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return ("—".to_string(), "—".to_string());
    };
    let h24 = dt.hour();
    let h12 = match h24 % 12 { 0 => 12, h => h };
    let ampm = if h24 >= 12 { "PM" } else { "AM" };
    let time_str = format!("{}:{:02} {}", h12, dt.minute(), ampm);
    let date_str = format!(
        "{} {}, {}",
        MONTHS[(dt.month0()) as usize],
        dt.day(),
        dt.year()
    );
    (time_str, date_str)
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
