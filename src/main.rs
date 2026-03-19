use std::net::SocketAddr;

use phonetik::{server, Phonetik};
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "phonetik=info".parse().unwrap()),
        )
        .init();

    let engine = Phonetik::new();
    let word_count = engine.word_count();
    let app = server::router(engine);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1273);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Phonetik starting on :{port} [{word_count} words]");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
