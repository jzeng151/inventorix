CREATE TABLE scan_audits (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  tile_id         INTEGER NOT NULL REFERENCES tiles(id),
  branch_id       INTEGER NOT NULL REFERENCES branches(id),
  scanned_by      INTEGER NOT NULL REFERENCES users(id),
  scanned_by_name TEXT NOT NULL,
  mode            TEXT NOT NULL CHECK(mode IN ('restock_confirm', 'integrity_check')),
  qty_before      INTEGER,   -- DB qty at scan time (integrity_check only)
  qty_after       INTEGER,   -- coordinator-entered qty; NULL = confirmed correct
  corrected       INTEGER NOT NULL DEFAULT 0, -- 1 if qty was changed
  refill_id       INTEGER REFERENCES refill_requests(id), -- restock_confirm only
  scanned_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_scan_audits_branch ON scan_audits(branch_id, scanned_at);
