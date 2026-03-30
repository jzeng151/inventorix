use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefillRequest {
    pub id: i64,
    pub tile_id: i64,
    pub requested_by: i64,
    pub approved_by: Option<i64>,
    pub fulfilled_by: Option<i64>,
    pub qty_requested: i64,
    pub status: String,
    pub requested_at: String,
    pub approved_at: Option<String>,
    pub fulfilled_at: Option<String>,
    pub timer_expires_at: String,
}
