use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tile {
    pub id: i64,
    pub branch_id: i64,
    pub item_number: String,
    pub collection: Option<String>,
    pub gts_description: Option<String>,
    pub new_bin: Option<String>,
    pub qty: i64,
    pub overflow_rack: i64,
    pub order_number: Option<String>,
    pub notes: Option<String>,
    pub sample_coordinator_id: Option<i64>,
    pub sales_rep_id: Option<i64>,
    pub low_inventory_threshold: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Tile {
    /// Health status for color-coding in the UI.
    pub fn health(&self) -> TileHealth {
        if self.qty == 0 {
            TileHealth::Critical
        } else if self.qty <= self.low_inventory_threshold {
            TileHealth::Low
        } else {
            TileHealth::Healthy
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TileHealth {
    Critical,
    Low,
    Healthy,
}
