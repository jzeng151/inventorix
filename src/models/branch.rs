use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Branch {
    pub id: i64,
    pub name: String,
    pub territory: String,
    pub created_at: String,
}
