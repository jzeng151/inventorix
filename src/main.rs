use dotenvy::dotenv;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "inventorix_server=debug,tower_http=debug".into()),
        )
        .init();

    // BIND_ADDR overrides the full address; PORT overrides just the port.
    // Default: 127.0.0.1 (loopback) for dev. Set BIND_ADDR=0.0.0.0:3000 to
    // accept LAN connections from the mobile app.
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| {
        let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
        format!("127.0.0.1:{port}")
    });

    // Bind before spawn so the port is guaranteed ready when anything navigates to it
    let listener = TcpListener::bind(&addr)
        .await
        .expect("failed to bind port");

    inventorix_server::start_server(listener).await;
}
