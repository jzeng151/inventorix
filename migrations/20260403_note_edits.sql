CREATE TABLE note_edits (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    tile_id       INTEGER NOT NULL REFERENCES tiles(id),
    branch_id     INTEGER NOT NULL REFERENCES branches(id),
    old_note      TEXT,
    new_note      TEXT,
    edited_by     INTEGER NOT NULL REFERENCES users(id),
    edited_by_name TEXT NOT NULL,
    edited_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_note_edits_branch ON note_edits(branch_id, edited_at);
