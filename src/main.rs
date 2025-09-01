mod db;
mod upload;
mod admin;

use axum::{routing::{get, post, head}, Router, Extension};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use clap::Parser;
use byte_unit::Byte;

#[derive(Clone)]
pub struct AppConfig {
    pub max_file_size: u64,
    pub upload_dir: String,
}

#[derive(Parser)]
#[command(name = "drcv")]
#[command(about = "A resumable file upload server")]
struct Args {
    #[arg(long, default_value = "100GiB")]
    #[arg(help = "Maximum file size (e.g., 100GiB, 10TB, 500MB)")]
    max_file_size: String,
    
    #[arg(long, default_value = "8080")]
    #[arg(help = "Upload server port")]
    upload_port: u16,
    
    #[arg(long, default_value = "8081")]
    #[arg(help = "Admin server port")]
    admin_port: u16,
    
    #[arg(long, default_value = "./uploads")]
    #[arg(help = "Upload directory path")]
    upload_dir: String,
}

fn parse_file_size(size_str: &str) -> u64 {
    Byte::parse_str(size_str, true)
        .map(|b| b.as_u64())
        .unwrap_or_else(|_| {
            eprintln!("Invalid file size format: {}", size_str);
            std::process::exit(1);
        })
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let config = AppConfig {
        max_file_size: parse_file_size(&args.max_file_size),
        upload_dir: args.upload_dir.clone(),
    };
    
    println!("Max file size: {} bytes ({})", config.max_file_size, args.max_file_size);
    println!("Upload directory: {}", config.upload_dir);
    println!("Upload port: {}", args.upload_port);
    println!("Admin port: {}", args.admin_port);
    
    let pool: SqlitePool = db::init_pool().await;

    // 업로드 서버 (8080)
    let upload_app = Router::new()
        .route("/", get(|| async {
            axum::response::Html(include_str!("static/index.html"))
        }))
        .route("/upload", post(upload::handle_chunk_upload))
        .route("/upload", head(upload::handle_upload_head))
        .route("/heartbeat", post(upload::handle_heartbeat))
        .layer(Extension(config.clone()))
        .with_state(pool.clone());

    // 관리자 서버 (8081)
    let admin_app = Router::new()
        .route("/", get(|| async {
            axum::response::Html(include_str!("static/admin.html"))
        }))
        .route("/data", get(admin::admin_data))
        .route("/events", get(admin::admin_events))
        .layer(Extension(config.clone()))
        .with_state(pool.clone());

    let upload_listener = TcpListener::bind(format!("0.0.0.0:{}", args.upload_port)).await.unwrap();
    let admin_listener  = TcpListener::bind(format!("127.0.0.1:{}", args.admin_port)).await.unwrap();
    
    // ConnectInfo를 사용하기 위해 필요
    let upload_app = upload_app.into_make_service_with_connect_info::<SocketAddr>();
    let admin_app = admin_app.into_make_service();

    println!("▶️ drcv uploader running on http://0.0.0.0:{}", args.upload_port);
    println!("▶️ drcv admin    running on http://127.0.0.1:{}", args.admin_port);
    
    // 1분마다 오래된 업로드를 disconnected로 마크하는 백그라운드 작업
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60)); // 1분
        loop {
            interval.tick().await;
            db::mark_stale_uploads_disconnected(&pool_clone, 1).await; // 1분 이상 업데이트 없으면 disconnected
        }
    });

    let upload_task = tokio::spawn(async move {
        axum::serve(upload_listener, upload_app).await.unwrap();
    });
    let admin_task = tokio::spawn(async move {
        axum::serve(admin_listener, admin_app).await.unwrap();
    });

    let _ = tokio::join!(upload_task, admin_task);
}
