use axum::{routing::{get, post, head}, Router, Extension};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use crate::{upload, config::AppConfig};

pub async fn create_app(pool: &SqlitePool, config: &AppConfig, shutdown_tx: &tokio::sync::broadcast::Sender<()>) -> tokio::task::JoinHandle<()> {
    let router = Router::new()
        .route("/", get(|| async {
            axum::response::Html(include_str!("../static/index.html"))
        }))
        .route("/upload", post(upload::handle_chunk_upload))
        .route("/upload", head(upload::handle_upload_head))
        .route("/heartbeat", post(upload::handle_heartbeat))
        .layer(axum::extract::DefaultBodyLimit::max({
            let overhead: u64 = 1024 * 1024; // 1 MiB
            let max = config.chunk_size.saturating_add(overhead);
            max as usize
        }))
        .layer(Extension(config.clone()))
        .with_state(pool.clone());
    
    let listener = TcpListener::bind(format!("0.0.0.0:{}", config.upload_port)).await.unwrap();
    let service = router.into_make_service_with_connect_info::<SocketAddr>();
    
    let mut shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        axum::serve(listener, service)
            .with_graceful_shutdown(async move { let _ = shutdown_rx.recv().await; })
            .await
            .unwrap();
    })
}