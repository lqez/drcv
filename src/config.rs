use std::time::Duration;
use clap::Parser;
use byte_unit::Byte;

#[derive(Clone)]
pub struct AppConfig {
    pub max_file_size: u64,
    pub chunk_size: u64,
    pub upload_dir: String,
    pub upload_port: u16,
    pub admin_port: u16,
    pub cf_domain: String,
    
    pub upload_timeout: Duration,
    pub cleanup_interval: Duration,
    pub upload_stale_timeout: i64,
    pub client_stale_timeout: i64,
    pub shutdown_grace_period: Duration,
    pub default_page_size: i64,
}

#[derive(Parser)]
#[command(name = "drcv")]
#[command(about = "A resumable file upload server")]
pub struct Args {
    #[arg(long, default_value = "100GiB")]
    #[arg(help = "Maximum file size (e.g., 100GiB, 10TB, 500MB)")]
    pub max_file_size: String,
    
    #[arg(long, default_value = "4MiB")]
    #[arg(help = "Upload chunk size (e.g., 4MiB, 1MiB, 512KB)")]
    pub chunk_size: String,
    
    #[arg(long, default_value = "8080")]
    #[arg(help = "Upload server port (use different ports if multiple instances behind NAT)")]
    pub upload_port: u16,
    
    #[arg(long, default_value = "8081")]
    #[arg(help = "Admin server port")]
    pub admin_port: u16,
    
    #[arg(long, default_value = "./uploads")]
    #[arg(help = "Upload directory path")]
    pub upload_dir: String,
    
    #[arg(long, default_value = "drcv.app")]
    #[arg(help = "Cloudflare Tunnel domain root (e.g., drcv.app)")]
    pub cf_domain: String,
}

impl Args {
    pub fn to_config(&self) -> AppConfig {
        AppConfig {
            max_file_size: parse_file_size(&self.max_file_size),
            chunk_size: parse_file_size(&self.chunk_size),
            upload_dir: self.upload_dir.clone(),
            upload_port: self.upload_port,
            admin_port: self.admin_port,
            cf_domain: self.cf_domain.clone(),
            
            upload_timeout: Duration::from_secs(300),
            cleanup_interval: Duration::from_secs(10),
            upload_stale_timeout: 60,
            client_stale_timeout: 120,
            shutdown_grace_period: Duration::from_secs(3),
            default_page_size: 100,
        }
    }
}

fn parse_file_size(size_str: &str) -> u64 {
    Byte::parse_str(size_str, true)
        .map(|b| b.as_u64())
        .unwrap_or_else(|_| {
            eprintln!("Invalid file size format: {}", size_str);
            std::process::exit(1);
        })
}