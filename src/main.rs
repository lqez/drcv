mod db;
mod upload;
mod admin;
mod tunnels;
mod utils;
mod config;
mod apps;

use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use clap::Parser;
use log::{info, warn, error};
use config::Args;
use tunnels::{TunnelConfig, create_tunnel_provider};
use apps::{admin::TunnelInfo, upload::create_app as create_upload_app, admin::create_app as create_admin_app};

#[tokio::main]
async fn main() {
    let args = Args::parse();
    
    // Initialize logger with appropriate level
    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();
    
    let config = args.to_config();
    if args.verbose {
        args.print_config_info(&config);
    }
    
    let pool = initialize_database().await;
    let tunnel_info = Arc::new(RwLock::new(TunnelInfo { hostname: None }));  
    let tunnel_runner = setup_tunnel(&pool, &config, &tunnel_info).await;
    let shutdown_tx = start_background_tasks(&pool, &config, tunnel_runner);
    let upload_task = create_upload_app(&pool, &config, &shutdown_tx).await;
    let admin_task = create_admin_app(&pool, &config, &tunnel_info, &shutdown_tx).await;
    
    info!("DRCV is ready");

    let tunnel_info_read = tunnel_info.read().await;
    if let Some(hostname) = &tunnel_info_read.hostname {
        info!("  • Share: https://{}", hostname);
    }
    info!("  • Admin: http://127.0.0.1:{}", config.admin_port);
    info!("  • Upload dir: {}", config.upload_dir);
    
    let _ = tokio::join!(upload_task, admin_task);
}

async fn initialize_database() -> SqlitePool {
    db::init_pool().await.unwrap_or_else(|e| {
        error!("Failed to initialize database: {}", e);
        std::process::exit(1);
    })
}

async fn setup_tunnel(pool: &SqlitePool, config: &config::AppConfig, tunnel_info: &Arc<RwLock<TunnelInfo>>) -> Option<Box<dyn tunnels::TunnelRunner>> {
    let provider = match create_tunnel_provider(&config.tunnel_provider) {
        Ok(p) => p,
        Err(e) => {
            error!("⚠️  Failed to create tunnel provider: {}", e);
            error!("💡 Available providers: cloudflare");
            std::process::exit(1);
        }
    };
    
    let cfg = TunnelConfig { 
        hostname_root: config.tunnel_domain.clone(), 
        local_port: config.upload_port 
    };
    
    match provider.ensure(pool, &cfg).await {
        Ok(manager) => {
            let hostname = manager.hostname().to_string();
            {
                let mut info = tunnel_info.write().await;
                info.hostname = Some(hostname);
            }
            
            match manager.run().await {
                Ok(runner) => Some(runner),
                Err(e) => { 
                    warn!("⚠️  Failed to run tunnel: {}", e); 
                    None 
                }
            }
        }
        Err(_) => None
    }
}

fn start_background_tasks(pool: &SqlitePool, config: &config::AppConfig, tunnel_runner: Option<Box<dyn tunnels::TunnelRunner>>) -> tokio::sync::broadcast::Sender<()> {
    use tokio::sync::broadcast;
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let shutdown_tx_clone = shutdown_tx.clone();
    
    let config_shutdown = config.shutdown_grace_period;
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        info!("Shutting down…");
        if let Some(runner) = tunnel_runner { let _ = runner.shutdown().await; }
        let _ = shutdown_tx_clone.send(());
        tokio::time::sleep(config_shutdown).await;
        info!("Shutting down. Bye!");
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
    
    shutdown_tx
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
