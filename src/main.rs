mod db;
mod upload;
mod admin;
mod cf_tunnel;
mod utils;
mod config;

use axum::{routing::{get, post, head}, Router, Extension};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use clap::Parser;
use config::Args;



#[tokio::main]
async fn main() {
    let args = Args::parse();

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
    let config = args.to_config();
    
    println!("Max file size: {} bytes ({})", config.max_file_size, args.max_file_size);
    println!("Chunk size: {} bytes ({})", config.chunk_size, args.chunk_size);
    println!("Upload directory: {}", config.upload_dir);
    println!("Upload port: {}", config.upload_port);
    println!("Admin port: {}", config.admin_port);
    
    let pool: SqlitePool = db::init_pool().await.unwrap_or_else(|e| {
        eprintln!("Failed to initialize database: {}", e);
        std::process::exit(1);
    });

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

    use std::sync::Arc;
    use tokio::sync::RwLock;
    #[derive(Clone)]
    pub struct TunnelInfo {
        pub hostname: Option<String>,
    }
    let tunnel_info = Arc::new(RwLock::new(TunnelInfo { hostname: None }));

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

    let upload_listener = TcpListener::bind(format!("0.0.0.0:{}", config.upload_port)).await.unwrap();
    let admin_listener  = TcpListener::bind(format!("127.0.0.1:{}", config.admin_port)).await.unwrap();
    
    let upload_app = upload_app.into_make_service_with_connect_info::<SocketAddr>();
    let admin_app = admin_app.into_make_service();

    println!("▶️ drcv admin running on http://127.0.0.1:{} (localhost only)", config.admin_port);

    let tunnel_runner = {
        let cfg = cf_tunnel::CfTunnelConfig { hostname_root: config.cf_domain.clone(), local_port: config.upload_port };
        match cf_tunnel::CfTunnelManager::ensure(&pool, &cfg).await {
            Ok(manager) => {
                {
                    let mut info = tunnel_info.write().await;
                    info.hostname = Some(manager.hostname.clone());
                }
                println!("DRCV is ready");
                println!("  • Share: https://{}", manager.hostname);
                println!("  • Admin: http://127.0.0.1:{}", config.admin_port);
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

    use tokio::sync::broadcast;
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut shutdown_rx_upload = shutdown_tx.subscribe();
    let mut shutdown_rx_admin = shutdown_tx.subscribe();
    let shutdown_tx_clone = shutdown_tx.clone();
    let tunnel_runner_opt = tunnel_runner;
    let config_clone = config.clone();
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        println!("Shutting down…");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        if let Some(runner) = tunnel_runner_opt { let _ = runner.shutdown().await; }
        let _ = shutdown_tx_clone.send(());
        tokio::time::sleep(config_clone.shutdown_grace_period).await;
        println!("Shutting down. Bye!");
        std::process::exit(0);
    });
    
    let pool_clone = pool.clone();
    let config_clone = config.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config_clone.cleanup_interval);
        loop {
            interval.tick().await;
            db::mark_stale_uploads_disconnected(&pool_clone, config_clone.upload_stale_timeout).await;
            db::mark_stale_clients_disconnected(&pool_clone, config_clone.client_stale_timeout).await;
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
