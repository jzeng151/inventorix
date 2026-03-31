use inventorix_server::ws::manager::{ConnectionManager, WsEvent};
use tokio::sync::mpsc;

// ── Broadcast delivers an event to a connected sender ────────────────────────

#[tokio::test]
async fn broadcast_delivers_event_to_connected_sender() {
    let mgr = ConnectionManager::new();
    let (tx, mut rx) = mpsc::channel(8);
    mgr.add_connection(1, tx);

    mgr.broadcast(1, WsEvent::InventoryUpdate { tile_id: 42, new_qty: 7 });

    let event = rx.recv().await.expect("expected an event");
    assert!(
        matches!(event, WsEvent::InventoryUpdate { tile_id: 42, new_qty: 7 }),
        "received wrong event: {event:?}"
    );
}

// ── Broadcast to a different branch does not deliver ─────────────────────────

#[tokio::test]
async fn broadcast_is_branch_scoped() {
    let mgr = ConnectionManager::new();
    let (tx, mut rx) = mpsc::channel(1);
    mgr.add_connection(1, tx); // registered on branch 1

    mgr.broadcast(2, WsEvent::InventoryUpdate { tile_id: 1, new_qty: 0 }); // sent to branch 2

    // Channel should be empty — nothing delivered across branches
    assert!(rx.try_recv().is_err(), "cross-branch event must not be delivered");
}

// ── Dead connection is removed on the next broadcast ─────────────────────────

#[tokio::test]
async fn dead_connection_is_removed_on_broadcast() {
    let mgr = ConnectionManager::new();
    let (tx, rx) = mpsc::channel(1);
    mgr.add_connection(1, tx);
    assert_eq!(mgr.connection_count(1), 1);

    drop(rx); // close the receiving end → sender becomes dead

    mgr.broadcast(1, WsEvent::InventoryUpdate { tile_id: 1, new_qty: 0 });

    assert_eq!(mgr.connection_count(1), 0, "dead sender must be cleaned up");
}

// ── total_connections sums across all branches ───────────────────────────────

#[tokio::test]
async fn total_connections_sums_all_branches() {
    let mgr = ConnectionManager::new();

    let (tx1, _rx1) = mpsc::channel::<WsEvent>(1);
    let (tx2, _rx2) = mpsc::channel::<WsEvent>(1);
    let (tx3, _rx3) = mpsc::channel::<WsEvent>(1);

    mgr.add_connection(1, tx1);
    mgr.add_connection(1, tx2);
    mgr.add_connection(2, tx3);

    assert_eq!(mgr.total_connections(), 3);
    assert_eq!(mgr.connection_count(1), 2);
    assert_eq!(mgr.connection_count(2), 1);
}
