use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub branch_id: i64,
    pub name: String,
    pub role: String,
    pub email: String,
    pub password_hash: String,
    pub territory: Option<String>,
    pub created_at: String,
    pub is_active: i64,
}
