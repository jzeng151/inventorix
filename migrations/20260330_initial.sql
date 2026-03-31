-- Inventorix initial schema — all 6 tables in one migration.
-- Apply with: sqlx migrate run

CREATE TABLE branches (
  id         INTEGER PRIMARY KEY NOT NULL,
  name       TEXT NOT NULL,
  territory  TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE users (
  id            INTEGER PRIMARY KEY NOT NULL,
  branch_id     INTEGER NOT NULL REFERENCES branches(id),
  name          TEXT NOT NULL,
  role          TEXT NOT NULL CHECK(role IN ('admin', 'coordinator', 'sales_rep')),
  email         TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  territory     TEXT,
  created_at    TEXT NOT NULL DEFAULT (datetime('now')),
  is_active     INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE tiles (
  id                      INTEGER PRIMARY KEY NOT NULL,
  branch_id               INTEGER NOT NULL REFERENCES branches(id),
  item_number             TEXT NOT NULL,
  collection              TEXT,
  gts_description         TEXT,
  new_bin                 TEXT,
  qty                     INTEGER NOT NULL DEFAULT 0,
  overflow_rack           INTEGER NOT NULL DEFAULT 0,  -- boolean (0/1)
  order_number            TEXT,
  notes                   TEXT,
  sample_coordinator_id   INTEGER REFERENCES users(id),
  sales_rep_id            INTEGER REFERENCES users(id),
  low_inventory_threshold INTEGER NOT NULL DEFAULT 5,
  created_at              TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at              TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(branch_id, item_number)
);

-- Chat is scoped to a tile. No global chat. Permanent audit log — visible to Admin only.
-- Messages are NOT stored in this table at runtime; they are written to daily log files
-- on the network drive (CHAT_LOG_PATH). This table is kept for the tile-scoped message
-- count and last-message metadata shown in the inventory UI.
CREATE TABLE chat_messages (
  id         INTEGER PRIMARY KEY NOT NULL,
  tile_id    INTEGER NOT NULL REFERENCES tiles(id),
  sender_id  INTEGER NOT NULL REFERENCES users(id),
  message    TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE refill_requests (
  id               INTEGER PRIMARY KEY NOT NULL,
  tile_id          INTEGER NOT NULL REFERENCES tiles(id),
  requested_by     INTEGER NOT NULL REFERENCES users(id),
  approved_by      INTEGER REFERENCES users(id),
  fulfilled_by     INTEGER REFERENCES users(id),
  qty_requested    INTEGER NOT NULL,
  status           TEXT NOT NULL DEFAULT 'pending'
                   CHECK(status IN ('pending', 'approved', 'fulfilled', 'expired')),
  requested_at     TEXT NOT NULL DEFAULT (datetime('now')),
  approved_at      TEXT,
  fulfilled_at     TEXT,
  timer_expires_at TEXT NOT NULL  -- 48h from requested_at; persisted for restart recovery
);

-- Required for the 5-minute timer loop — prevents full table scan every run.
CREATE INDEX idx_refill_timer ON refill_requests(status, timer_expires_at);

-- Event log for future trend analytics (Phase 3). Populated from day 1.
CREATE TABLE inventory_events (
  id         INTEGER PRIMARY KEY NOT NULL,
  tile_id    INTEGER NOT NULL REFERENCES tiles(id),
  event_type TEXT NOT NULL CHECK(event_type IN ('import', 'manual_edit', 'refill_fulfilled')),
  old_qty    INTEGER NOT NULL,
  new_qty    INTEGER NOT NULL,
  user_id    INTEGER NOT NULL REFERENCES users(id),
  notes      TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Seed: NYC branch
INSERT INTO branches (name, territory) VALUES ('NYC Showroom', 'New York City');
