use axum::{routing::get, Router, Extension};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::TcpListener;
use crate::{admin, config::AppConfig};

#[derive(Clone)]
pub struct TunnelInfo {
    pub hostname: Option<String>,
}

pub async fn create_app(pool: &SqlitePool, config: &AppConfig, tunnel_info: &Arc<RwLock<TunnelInfo>>, shutdown_tx: &tokio::sync::broadcast::Sender<()>) -> tokio::task::JoinHandle<()> {
    let router = Router::new()
        .route("/", get(|| async {
            axum::response::Html(include_str!("../static/admin.html"))
        }))
        .route("/data", get(admin::admin_data))
        .route("/clients", get(admin::admin_clients))
        .route("/tunnel", get({
            let tunnel_info = Arc::clone(tunnel_info);
            move |_: axum::extract::State<SqlitePool>| async move {
                let info = tunnel_info.read().await;
                axum::Json(serde_json::json!({ "hostname": info.hostname }))
            }
        }))
        .route("/events", get(admin::admin_events))
        .layer(Extension(config.clone()))
        .with_state(pool.clone());
    
    let listener = TcpListener::bind(format!("127.0.0.1:{}", config.admin_port)).await.unwrap();
    let service = router.into_make_service();
    
    let mut shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        axum::serve(listener, service)
            .with_graceful_shutdown(async move { let _ = shutdown_rx.recv().await; })
            .await
            .unwrap();
    })
}