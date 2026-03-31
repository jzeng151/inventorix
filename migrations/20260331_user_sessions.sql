-- Maps user_id → session_id so admin can invalidate all sessions for a user on deactivation.
-- Populated on login, cleaned up on logout and deactivation.
CREATE TABLE user_sessions (
  session_id TEXT    NOT NULL PRIMARY KEY,
  user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_user_sessions_user ON user_sessions(user_id);
