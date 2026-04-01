-- SQLite does not support ALTER COLUMN — recreate table to add 'rejected' status,
-- plus rejected_by / rejected_at columns for audit trail.

PRAGMA foreign_keys = OFF;

CREATE TABLE refill_requests_new (
  id               INTEGER PRIMARY KEY NOT NULL,
  tile_id          INTEGER NOT NULL REFERENCES tiles(id),
  requested_by     INTEGER NOT NULL REFERENCES users(id),
  approved_by      INTEGER REFERENCES users(id),
  fulfilled_by     INTEGER REFERENCES users(id),
  rejected_by      INTEGER REFERENCES users(id),
  qty_requested    INTEGER NOT NULL,
  status           TEXT NOT NULL DEFAULT 'pending'
                   CHECK(status IN ('pending', 'approved', 'fulfilled', 'expired', 'rejected')),
  requested_at     TEXT NOT NULL DEFAULT (datetime('now')),
  approved_at      TEXT,
  fulfilled_at     TEXT,
  rejected_at      TEXT,
  timer_expires_at TEXT NOT NULL
);

INSERT INTO refill_requests_new
  SELECT id, tile_id, requested_by, approved_by, fulfilled_by,
         NULL, qty_requested, status, requested_at, approved_at, fulfilled_at,
         NULL, timer_expires_at
  FROM refill_requests;

DROP INDEX IF EXISTS idx_refill_timer;
DROP TABLE refill_requests;
ALTER TABLE refill_requests_new RENAME TO refill_requests;

CREATE INDEX idx_refill_timer ON refill_requests(status, timer_expires_at);

PRAGMA foreign_keys = ON;
