use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChatMessage {
    pub id: i64,
    pub tile_id: i64,
    pub sender_id: i64,
    pub message: String,
    pub created_at: String,
}
