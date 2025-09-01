mod db;
mod upload;
mod admin;
mod tunnel;

use axum::{routing::{get, post, head}, Router, Extension};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use clap::Parser;
use byte_unit::Byte;
use std::net::{IpAddr, Ipv4Addr};

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
    #[arg(help = "Upload server port (use different ports if multiple instances behind NAT)")]
    upload_port: u16,
    
    #[arg(long, default_value = "8081")]
    #[arg(help = "Admin server port")]
    admin_port: u16,
    
    #[arg(long, default_value = "./uploads")]
    #[arg(help = "Upload directory path")]
    upload_dir: String,
    
    #[arg(long)]
    #[arg(help = "Enable tunnel mode to expose server via drcv.app subdomain")]
    tunnel: bool,
    
    #[arg(long, default_value = "https://api.drcv.app")]
    #[arg(help = "Tunnel server URL")]
    tunnel_server: String,
}

fn parse_file_size(size_str: &str) -> u64 {
    Byte::parse_str(size_str, true)
        .map(|b| b.as_u64())
        .unwrap_or_else(|_| {
            eprintln!("Invalid file size format: {}", size_str);
            std::process::exit(1);
        })
}

fn get_local_ips() -> Vec<String> {
    let mut local_ips = Vec::new();
    
    // 네트워크 인터페이스에서 로컬 IP 주소들 수집
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (_name, ip) in interfaces {
            match ip {
                IpAddr::V4(ipv4) => {
                    // 루프백, 링크로컬, APIPA 주소 제외
                    if !ipv4.is_loopback() && !ipv4.is_link_local() && !is_apipa(&ipv4) {
                        local_ips.push(format!("{}", ipv4));
                    }
                }
                _ => {} // IPv6는 일단 제외
            }
        }
    }
    
    // 대안: 간단한 소켓 연결 시도로 로컬 IP 감지
    if local_ips.is_empty() {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(local_addr) = socket.local_addr() {
                    local_ips.push(local_addr.ip().to_string());
                }
            }
        }
    }
    
    local_ips
}

fn is_apipa(ip: &Ipv4Addr) -> bool {
    // APIPA range: 169.254.0.0/16
    let octets = ip.octets();
    octets[0] == 169 && octets[1] == 254
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

    // 로컬 IP 주소들 감지
    let local_ips = get_local_ips();
    
    println!("▶️ drcv uploader running on:");
    println!("   • http://0.0.0.0:{} (all interfaces)", args.upload_port);
    for ip in &local_ips {
        println!("   • http://{}:{} (local network)", ip, args.upload_port);
    }
    
    println!("▶️ drcv admin running on http://127.0.0.1:{} (localhost only)", args.admin_port);
    
    // 터널 모드 활성화 시 터널 클라이언트 시작
    if args.tunnel {
        let mut tunnel_client = tunnel::TunnelClient::new(args.upload_port, args.tunnel_server.clone());
        match tunnel_client.register().await {
            Ok(subdomain) => {
                println!("🔗 Tunnel active: {}.drcv.app", subdomain);
                tunnel_client.print_status();
                
                // Keep-alive 시작
                if let Err(e) = tunnel_client.start_keepalive().await {
                    println!("⚠️  Keepalive setup failed: {}", e);
                }
            }
            Err(e) => {
                eprintln!("❌ Tunnel setup failed: {}", e);
                eprintln!("💡 Continuing in local mode...");
            }
        }
    }
    
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
