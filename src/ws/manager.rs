// Lane C: ConnectionManager — branch-scoped WebSocket broadcast
// TODO: implement DashMap<branch_id, Vec<Sender<WsEvent>>> + broadcast logic

use dashmap::DashMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    ChatMessage { sender_name: String, role: String, message: String },
    InventoryUpdate { tile_id: i64, new_qty: i64 },
    RefillRequested { refill_id: i64, tile_id: i64, item_number: String },
    RefillStatusChange { refill_id: i64, status: String },
}

pub struct ConnectionManager {
    connections: DashMap<i64, Vec<mpsc::Sender<WsEvent>>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Broadcast an event to all connections in a branch.
    /// Dead senders are cleaned up silently — no panic on closed channel.
    pub fn broadcast(&self, branch_id: i64, event: WsEvent) {
        if let Some(mut senders) = self.connections.get_mut(&branch_id) {
            senders.retain(|tx| tx.try_send(event.clone()).is_ok());
        }
    }

    pub fn add_connection(&self, branch_id: i64, tx: mpsc::Sender<WsEvent>) {
        self.connections.entry(branch_id).or_default().push(tx);
    }

    pub fn connection_count(&self, branch_id: i64) -> usize {
        self.connections.get(&branch_id).map_or(0, |v| v.len())
    }

    pub fn total_connections(&self) -> usize {
        self.connections.iter().map(|e| e.value().len()).sum()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
