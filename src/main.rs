mod db;
mod upload;
mod admin;
mod cf_tunnel;

use axum::{routing::{get, post, head}, Router, Extension};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use clap::Parser;
use byte_unit::Byte;

#[derive(Clone)]
pub struct AppConfig {
    pub max_file_size: u64,
    pub chunk_size: u64,
    pub upload_dir: String,
}

#[derive(Parser)]
#[command(name = "drcv")]
#[command(about = "A resumable file upload server")]
struct Args {
    #[arg(long, default_value = "100GiB")]
    #[arg(help = "Maximum file size (e.g., 100GiB, 10TB, 500MB)")]
    max_file_size: String,
    
    #[arg(long, default_value = "4MiB")]
    #[arg(help = "Upload chunk size (e.g., 4MiB, 1MiB, 512KB)")]
    chunk_size: String,
    
    #[arg(long, default_value = "8080")]
    #[arg(help = "Upload server port (use different ports if multiple instances behind NAT)")]
    upload_port: u16,
    
    #[arg(long, default_value = "8081")]
    #[arg(help = "Admin server port")]
    admin_port: u16,
    
    #[arg(long, default_value = "./uploads")]
    #[arg(help = "Upload directory path")]
    upload_dir: String,
    #[arg(long, default_value = "drcv.app")]
    #[arg(help = "Cloudflare Tunnel domain root (e.g., drcv.app)")]
    cf_domain: String,
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

    // Preflight: require cloudflared to be installed
    if std::process::Command::new("cloudflared").arg("--version").output().is_err() {
        eprintln!("❌ cloudflared not found in PATH.");
        eprintln!("➡️  Please install and authenticate Cloudflare Tunnel, then re-run drcv.");
        if cfg!(target_os = "macos") {
            eprintln!("   • macOS: brew install cloudflared");
        }
        eprintln!("   • Docs: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/install-and-setup/installation");
        eprintln!("   • Login (once): cloudflared tunnel login");
        std::process::exit(1);
    }
    let config = AppConfig {
        max_file_size: parse_file_size(&args.max_file_size),
        chunk_size: parse_file_size(&args.chunk_size),
        upload_dir: args.upload_dir.clone(),
    };
    
    println!("Max file size: {} bytes ({})", config.max_file_size, args.max_file_size);
    println!("Chunk size: {} bytes ({})", config.chunk_size, args.chunk_size);
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
        .layer(axum::extract::DefaultBodyLimit::max({
            let overhead: u64 = 1024 * 1024; // 1 MiB
            let max = config.chunk_size.saturating_add(overhead);
            max as usize
        }))
        .layer(Extension(config.clone()))
        .with_state(pool.clone());

    // 터널 정보를 위한 공유 상태
    use std::sync::Arc;
    use tokio::sync::RwLock;
    #[derive(Clone)]
    pub struct TunnelInfo {
        pub hostname: Option<String>,
    }
    let tunnel_info = Arc::new(RwLock::new(TunnelInfo { hostname: None }));

    // 관리자 서버 (8081)
    let admin_app = Router::new()
        .route("/", get(|| async {
            axum::response::Html(include_str!("static/admin.html"))
        }))
        .route("/data", get(admin::admin_data))
        .route("/clients", get(admin::admin_clients))
        .route("/tunnel", get({
            let tunnel_info = tunnel_info.clone();
            move |_: axum::extract::State<SqlitePool>| async move {
                let info = tunnel_info.read().await;
                axum::Json(serde_json::json!({ "hostname": info.hostname }))
            }
        }))
        .route("/events", get(admin::admin_events))
        .layer(Extension(config.clone()))
        .with_state(pool.clone());

    let upload_listener = TcpListener::bind(format!("0.0.0.0:{}", args.upload_port)).await.unwrap();
    let admin_listener  = TcpListener::bind(format!("127.0.0.1:{}", args.admin_port)).await.unwrap();
    
    // ConnectInfo를 사용하기 위해 필요
    let upload_app = upload_app.into_make_service_with_connect_info::<SocketAddr>();
    let admin_app = admin_app.into_make_service();

    println!("▶️ drcv admin running on http://127.0.0.1:{} (localhost only)", args.admin_port);

    // Cloudflare Tunnel 자동 생성/실행
    let tunnel_runner = {
        let cfg = cf_tunnel::CfTunnelConfig { hostname_root: args.cf_domain.clone(), local_port: args.upload_port };
        match cf_tunnel::CfTunnelManager::ensure(&pool, &cfg).await {
            Ok(manager) => {
                {
                    let mut info = tunnel_info.write().await;
                    info.hostname = Some(manager.hostname.clone());
                }
                println!("DRCV is ready");
                println!("  • Share: https://{}", manager.hostname);
                println!("  • Admin: http://127.0.0.1:{}", args.admin_port);
                println!("  • Upload dir: {}", config.upload_dir);
                match manager.run().await {
                    Ok(runner) => Some(runner),
                    Err(e) => { eprintln!("⚠️  Failed to run cloudflared: {}", e); None }
                }
            }
            Err(e) => {
                eprintln!("⚠️  Cloudflare Tunnel not started: {}", e);
                eprintln!("💡 Ensure cloudflared is installed and logged in (cloudflared tunnel login)");
                None
            }
        }
    };

    // Graceful shutdown wiring for Axum servers and tunnel runner
    use tokio::sync::broadcast;
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut shutdown_rx_upload = shutdown_tx.subscribe();
    let mut shutdown_rx_admin = shutdown_tx.subscribe();
    let shutdown_tx_clone = shutdown_tx.clone();
    let tunnel_runner_opt = tunnel_runner;
    tokio::spawn(async move {
        // Wait for Ctrl-C or 'q'\n
        wait_for_shutdown_signal().await;
        // Print and flush shutdown message immediately before cleanup
        println!("Shutting down…");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        // Stop cloudflared if running
        if let Some(runner) = tunnel_runner_opt { let _ = runner.shutdown().await; }
        // Notify servers to shutdown
        let _ = shutdown_tx_clone.send(());
        // Give servers time to drain persistent connections (e.g., SSE)
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        // Force exit if anything is still hanging
        println!("Shutting down. Bye!");
        std::process::exit(0);
    });
    
    // 1분마다 오래된 업로드/클라이언트를 disconnected로 마크하는 백그라운드 작업
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60)); // 1분
        loop {
            interval.tick().await;
            db::mark_stale_uploads_disconnected(&pool_clone, 1).await; // 1분 이상 업데이트 없으면 disconnected
            db::mark_stale_clients_disconnected(&pool_clone, 2).await; // 2분 이상 heartbeat 없으면 disconnected
        }
    });

    let upload_task = tokio::spawn(async move {
        axum::serve(upload_listener, upload_app)
            .with_graceful_shutdown(async move { let _ = shutdown_rx_upload.recv().await; })
            .await
            .unwrap();
    });
    let admin_task = tokio::spawn(async move {
        axum::serve(admin_listener, admin_app)
            .with_graceful_shutdown(async move { let _ = shutdown_rx_admin.recv().await; })
            .await
            .unwrap();
    });

    let _ = tokio::join!(upload_task, admin_task);
}

async fn wait_for_shutdown_signal() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::signal;

    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    let stdin_quit = async {
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if line.trim().eq_ignore_ascii_case("q") { break; }
                }
                Err(_e) => break,
            }
        }
    };

    tokio::select! { _ = ctrl_c => {}, _ = stdin_quit => {} }
}
