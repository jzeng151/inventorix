use tokio::net::TcpListener;

/// Opens the OS native file picker filtered to .xlsx files.
/// Returns the selected path as a String, or `None` if the user cancelled.
#[tauri::command]
fn pick_excel_file(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .add_filter("Excel workbook", &["xlsx"])
        .blocking_pick_file()
        .map(|p| p.to_string())
}

/// Opens the given filesystem path in the OS default application (browser for .html).
#[tauri::command]
fn open_digest_in_browser(app: tauri::AppHandle, path: String) {
    use tauri_plugin_opener::OpenerExt;
    if let Err(e) = app.opener().open_path(&path, None::<&str>) {
        tracing::error!("opener failed: {e}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());

    // Barcode scanner plugin is mobile-only — the entire crate is #[cfg(mobile)].
    // On Android, scanning is driven by the plugin's Android IPC directly from JS
    // via window.__TAURI__.core.invoke('plugin:barcode-scanner|scan', ...).
    // No Rust wrapper command is needed.
    #[cfg(target_os = "android")]
    {
        builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    }

    // On desktop (Windows) spawn the embedded Axum server.
    // On Android the WebView connects to the Windows machine's LAN IP instead —
    // no local server needed.
    #[cfg(not(target_os = "android"))]
    {
        builder = builder.setup(|_app| {
            tauri::async_runtime::spawn(async {
                // BIND_ADDR overrides address; PORT overrides just the port.
                let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| {
                    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
                    format!("127.0.0.1:{port}")
                });
                let listener = TcpListener::bind(&addr)
                    .await
                    .expect("failed to bind port");
                inventorix_server::start_server(listener).await;
            });
            Ok(())
        });
    }

    builder
        .invoke_handler(tauri::generate_handler![
            pick_excel_file,
            open_digest_in_browser,
        ])
        .run(tauri::generate_context!())
        .expect("error while running inventorix");
}
