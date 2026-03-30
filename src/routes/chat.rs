// Lane C: POST /tiles/:id/chat — send message, write to daily log file, broadcast via WS
// Access: any authenticated user. History view: admin only.
// Log format: one file per day on network drive (CHAT_LOG_PATH env var).
// Purge: daily tokio::spawn loop deletes files older than 30 days by filename date.
// TODO: implement chat send, log write, admin history route, purge job
