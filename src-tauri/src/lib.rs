use tokio::net::TcpListener;

/// Tauri IPC stub — Lane E Day 2.
/// Returns a hardcoded path until the real FileDialogBuilder lands (Day 3-4).
#[tauri::command]
fn pick_excel_file() -> String {
    // TODO (Lane E Day 3-4): replace with tauri_plugin_dialog file picker
    "C:\\placeholder\\NYC Showroom.xlsx".to_string()
}

/// Tauri IPC stub — Lane E Day 2.
/// No-op until the real opener::open call lands (Day 3-4).
#[tauri::command]
fn open_digest_in_browser(_path: String) {
    // TODO (Lane E Day 3-4): replace with tauri_plugin_opener::open_url
}

pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            // Spawn the Axum server inside the Tauri process.
            // TcpListener::bind runs before spawn — port is guaranteed ready
            // when WebView2 navigates to it.
            tauri::async_runtime::spawn(async {
                let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
                let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
                    .await
                    .expect("failed to bind port");
                inventorix_server::start_server(listener).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![pick_excel_file, open_digest_in_browser])
        .run(tauri::generate_context!())
        .expect("error while running inventorix");
}
