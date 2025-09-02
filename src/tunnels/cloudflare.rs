use crate::{db, utils};
use super::{TunnelProvider, TunnelManager, TunnelRunner, TunnelConfig, TunnelError};
use async_trait::async_trait;
use std::path::{PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use rand::{distributions::Alphanumeric, Rng};
use sqlx::SqlitePool;

pub struct CloudflareTunnelProvider;

#[async_trait]
impl TunnelProvider for CloudflareTunnelProvider {
    async fn ensure(&self, pool: &SqlitePool, config: &TunnelConfig) -> Result<Box<dyn TunnelManager>, TunnelError> {
        check_cloudflared().await?;

        let hash = if let Some(h) = db::kv_get(pool, "cf_hash").await { 
            h 
        } else {
            let h: String = rand_hash(6);
            db::kv_set(pool, "cf_hash", &h).await;
            h
        };
        
        let hostname = format!("{}.{}", hash, config.hostname_root);
        let tunnel_name = format!("drcv-{}", hash);

        let mut uuid = get_tunnel_uuid(&tunnel_name).await?;
        if uuid.is_none() {
            create_tunnel(&tunnel_name).await?;
            uuid = get_tunnel_uuid(&tunnel_name).await?;
        }
        let uuid = uuid.ok_or_else(|| TunnelError::ConfigError("Failed to obtain tunnel UUID".to_string()))?;

        route_dns(&tunnel_name, &hostname).await?;
        let config_path = write_config(&uuid, &hostname, config.local_port).await?;

        Ok(Box::new(CloudflareTunnelManager { hostname, config_path }))
    }
}

struct CloudflareTunnelManager {
    hostname: String,
    config_path: PathBuf,
}

#[async_trait]
impl TunnelManager for CloudflareTunnelManager {
    fn hostname(&self) -> &str {
        &self.hostname
    }

    async fn run(&self) -> Result<Box<dyn TunnelRunner>, TunnelError> {
        let cfg = &self.config_path;
        let mut child = Command::new("cloudflared")
            .args(["--loglevel", "error", "--transport-loglevel", "error", "tunnel", "--config"]) 
            .arg(cfg)
            .arg("run")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| TunnelError::NetworkError(format!("failed to start cloudflared: {}", e)))?;

        let mut child_stderr = child.stderr.take();
        tokio::spawn(async move {
            if let Some(mut err) = child_stderr.take() {
                let _ = tokio::io::copy(&mut err, &mut tokio::io::stderr()).await;
            }
        });

        Ok(Box::new(CloudflareTunnelRunner { child }))
    }
}

struct CloudflareTunnelRunner {
    child: tokio::process::Child,
}

#[async_trait]
impl TunnelRunner for CloudflareTunnelRunner {
    async fn shutdown(mut self: Box<Self>) -> Result<(), TunnelError> {
        self.child.kill().await
            .map_err(|e| TunnelError::NetworkError(format!("failed to stop cloudflared: {}", e)))
    }
}

async fn check_cloudflared() -> Result<(), TunnelError> {
    let status = Command::new("cloudflared")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
        
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => {
            eprintln!("❌ cloudflared installation corrupted.");
            print_cloudflared_install_guide();
            Err(TunnelError::NotInstalled("cloudflared --version failed".to_string()))
        },
        Err(_) => {
            eprintln!("❌ cloudflared not found in PATH.");
            print_cloudflared_install_guide();
            Err(TunnelError::NotInstalled("cloudflared not found in PATH".to_string()))
        }
    }
}

fn print_cloudflared_install_guide() {
    eprintln!("➡️  Please install and authenticate Cloudflare Tunnel:");
    eprintln!();
    eprintln!("📦 Installation:");
    if cfg!(target_os = "macos") {
        eprintln!("   brew install cloudflared");
    } else if cfg!(target_os = "linux") {
        eprintln!("   # Debian/Ubuntu:");
        eprintln!("   curl -L --output cloudflared.deb https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb");
        eprintln!("   sudo dpkg -i cloudflared.deb");
        eprintln!();
        eprintln!("   # Other Linux:");
        eprintln!("   curl -L --output /usr/local/bin/cloudflared https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64");
        eprintln!("   chmod +x /usr/local/bin/cloudflared");
    } else if cfg!(target_os = "windows") {
        eprintln!("   # Download from: https://github.com/cloudflare/cloudflared/releases/latest");
        eprintln!("   # Or use Chocolatey: choco install cloudflared");
    }
    eprintln!();
    eprintln!("🔑 Authentication (required once):");
    eprintln!("   cloudflared tunnel login");
    eprintln!();
    eprintln!("📖 Documentation:");
    eprintln!("   https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/install-and-setup/installation");
    eprintln!();
}

fn rand_hash(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .map(|c| (c as char).to_ascii_lowercase())
        .take(len)
        .collect()
}

async fn create_tunnel(name: &str) -> Result<(), TunnelError> {
    let out = Command::new("cloudflared")
        .args(["tunnel", "create", name])
        .output()
        .await
        .map_err(|e| TunnelError::NetworkError(format!("failed to exec cloudflared tunnel create: {}", e)))?;
        
    if !out.status.success() {
        let stderr = utils::bytes_to_string(&out.stderr);
        if stderr.to_lowercase().contains("not authenticated") || stderr.to_lowercase().contains("login") {
            eprintln!("❌ Cloudflare Tunnel not authenticated.");
            eprintln!("🔑 Please run: cloudflared tunnel login");
            eprintln!("📖 Follow the browser authentication flow, then re-run drcv.");
            return Err(TunnelError::AuthError("Not authenticated with Cloudflare".to_string()));
        }
        return Err(TunnelError::ConfigError(format!("create tunnel failed: {}", stderr)));
    }
    Ok(())
}

async fn get_tunnel_uuid(name: &str) -> Result<Option<String>, TunnelError> {
    let out = Command::new("cloudflared")
        .args(["tunnel", "list"])
        .output()
        .await
        .map_err(|e| TunnelError::NetworkError(format!("failed to exec cloudflared tunnel list: {}", e)))?;
        
    if !out.status.success() {
        let stderr = utils::bytes_to_string(&out.stderr);
        if stderr.to_lowercase().contains("not authenticated") || stderr.to_lowercase().contains("login") {
            eprintln!("❌ Cloudflare Tunnel not authenticated.");
            eprintln!("🔑 Please run: cloudflared tunnel login");
            return Err(TunnelError::AuthError("Not authenticated with Cloudflare".to_string()));
        }
        return Err(TunnelError::ConfigError(format!("tunnel list failed: {}", stderr)));
    }
    
    let stdout = utils::bytes_to_string(&out.stdout);
    for line in stdout.lines() {
        if line.contains(name) {
            if let Some(id) = extract_uuid(line) { 
                return Ok(Some(id)); 
            }
        }
    }
    Ok(None)
}

fn extract_uuid(s: &str) -> Option<String> {
    let uuid_re = &regex_uuid::UUID_RE;
    uuid_re.find(s).map(|m| m.as_str().to_string())
}

mod regex_uuid {
    use once_cell::sync::Lazy;
    use regex::Regex;
    pub static UUID_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}").unwrap()
    });
}

async fn route_dns(name: &str, hostname: &str) -> Result<(), TunnelError> {
    let out = Command::new("cloudflared")
        .args(["tunnel", "route", "dns", name, hostname])
        .output()
        .await
        .map_err(|e| TunnelError::NetworkError(format!("failed to exec cloudflared tunnel route dns: {}", e)))?;
        
    if !out.status.success() {
        let err = utils::bytes_to_string(&out.stderr);
        if err.to_lowercase().contains("not authenticated") || err.to_lowercase().contains("login") {
            eprintln!("❌ Cloudflare Tunnel not authenticated.");
            eprintln!("🔑 Please run: cloudflared tunnel login");
            return Err(TunnelError::AuthError("Not authenticated with Cloudflare".to_string()));
        }
        if !err.to_lowercase().contains("already exists") {
            return Err(TunnelError::ConfigError(format!("route dns failed: {}", err)));
        }
    }
    Ok(())
}

async fn write_config(uuid: &str, hostname: &str, port: u16) -> Result<PathBuf, TunnelError> {
    let home = dirs::home_dir().ok_or_else(|| TunnelError::ConfigError("cannot resolve home directory".to_string()))?;
    let cfg_dir = home.join(".cloudflared");
    let cfg_path = cfg_dir.join(format!("config-{}.yml", hostname));
    let creds = cfg_dir.join(format!("{}.json", uuid));
    let content = format!(
        "tunnel: {uuid}\ncredentials-file: {creds}\n\ningress:\n  - hostname: {host}\n    service: http://localhost:{port}\n  - service: http_status:404\n",
        uuid = uuid,
        creds = creds.display(),
        host = hostname,
        port = port,
    );
    
    tokio::fs::create_dir_all(&cfg_dir).await
        .map_err(|e| TunnelError::ConfigError(e.to_string()))?;
    tokio::fs::write(&cfg_path, content).await
        .map_err(|e| TunnelError::ConfigError(e.to_string()))?;
    Ok(cfg_path)
}