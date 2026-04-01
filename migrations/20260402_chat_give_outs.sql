CREATE TABLE chat_give_outs (
  id               INTEGER PRIMARY KEY NOT NULL,
  branch_id        INTEGER NOT NULL REFERENCES branches(id),
  item_number      TEXT NOT NULL,
  handled_by_name  TEXT NOT NULL,
  handled_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Fast lookup when rendering chat history for a branch
CREATE INDEX idx_chat_give_outs_branch ON chat_give_outs(branch_id, item_number);
