mod db;
mod upload;
mod admin;

use axum::{routing::{get, post}, Router};
use sqlx::SqlitePool;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let pool: SqlitePool = db::init_pool().await;

    // 업로드 서버 (8080)
    let upload_app = Router::new()
        .route("/", get(|| async {
            axum::response::Html(include_str!("static/index.html"))
        }))
        .route("/upload", post(upload::handle_chunk_upload))
        .with_state(pool.clone());

    // 관리자 서버 (8081)
    let admin_app = Router::new()
        .route("/", get(|| async {
            axum::response::Html(include_str!("static/admin.html"))
        }))
        .route("/data", get(admin::admin_data))
        .with_state(pool.clone());

    let upload_listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    let admin_listener  = TcpListener::bind("127.0.0.1:8081").await.unwrap();

    println!("▶️ drcv uploader running on http://127.0.0.1:8080");
    println!("▶️ drcv admin    running on http://127.0.0.1:8081");

    let upload_task = tokio::spawn(async move {
        axum::serve(upload_listener, upload_app).await.unwrap();
    });
    let admin_task = tokio::spawn(async move {
        axum::serve(admin_listener, admin_app).await.unwrap();
    });

    let _ = tokio::join!(upload_task, admin_task);
}
