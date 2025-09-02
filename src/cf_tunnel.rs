use std::path::{PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use rand::{distributions::Alphanumeric, Rng};
use crate::{db, utils};
use sqlx::SqlitePool;

pub struct CfTunnelConfig {
    pub hostname_root: String, // e.g., "drcv.app"
    pub local_port: u16,       // e.g., 8080
}

pub struct CfTunnelManager {
    pub hostname: String,
    config_path: PathBuf,
}

pub struct CfTunnelRunner {
    child: tokio::process::Child,
}

impl CfTunnelManager {
    pub async fn ensure(pool: &SqlitePool, cfg: &CfTunnelConfig) -> Result<Self, String> {
        // 1) Ensure cloudflared exists
        check_cloudflared().await?;

        // 2) Load or create hash
        let hash = if let Some(h) = db::kv_get(pool, "cf_hash").await { h } else {
            let h: String = rand_hash(6);
            db::kv_set(pool, "cf_hash", &h).await;
            h
        };
        let hostname = format!("{}.{root}", hash, root = cfg.hostname_root);
        let tunnel_name = format!("drcv-{}", hash);

        // 3) Create or fetch tunnel UUID
        let mut uuid = get_tunnel_uuid(&tunnel_name).await?;
        if uuid.is_none() {
            create_tunnel(&tunnel_name).await?;
            uuid = get_tunnel_uuid(&tunnel_name).await?;
        }
        let uuid = uuid.ok_or_else(|| "Failed to obtain tunnel UUID".to_string())?;

        // 4) Route DNS to hostname
        route_dns(&tunnel_name, &hostname).await?;

        // 5) Write a minimal config for ingress
        let config_path = write_config(&uuid, &hostname, cfg.local_port).await?;

        Ok(Self { hostname, config_path })
    }

    pub async fn run(&self) -> Result<CfTunnelRunner, String> {
        let cfg = &self.config_path;
        let mut child = Command::new("cloudflared")
            .args(["--loglevel", "error", "--transport-loglevel", "error", "tunnel", "--config"]) 
            .arg(cfg)
            .arg("run")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to start cloudflared: {}", e))?;

        // Pipe only stderr (errors) without moving `child`
        let mut child_stderr = child.stderr.take();
        tokio::spawn(async move {
            if let Some(mut err) = child_stderr.take() {
                let _ = tokio::io::copy(&mut err, &mut tokio::io::stderr()).await;
            }
        });

        Ok(CfTunnelRunner { child })
    }
}

impl CfTunnelRunner {
    pub async fn shutdown(mut self) -> Result<(), String> {
        // Try graceful kill
        if let Err(e) = self.child.kill().await {
            return Err(format!("failed to stop cloudflared: {}", e));
        }
        Ok(())
    }
}

async fn check_cloudflared() -> Result<(), String> {
    let status = Command::new("cloudflared")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|e| format!("cloudflared not found: {}", e))?;
    if !status.success() { return Err("cloudflared --version failed".to_string()); }
    Ok(())
}

fn rand_hash(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .map(|c| (c as char).to_ascii_lowercase())
        .take(len)
        .collect()
}

async fn create_tunnel(name: &str) -> Result<(), String> {
    let out = Command::new("cloudflared")
        .args(["tunnel", "create", name])
        .output()
        .await
        .map_err(|e| format!("failed to exec cloudflared tunnel create: {}", e))?;
    if !out.status.success() {
        return Err(format!("create tunnel failed: {}", utils::bytes_to_string(&out.stderr)));
    }
    Ok(())
}

async fn get_tunnel_uuid(name: &str) -> Result<Option<String>, String> {
    // Try JSON output if available
    let out = Command::new("cloudflared")
        .args(["tunnel", "list"]) // `--output json` not available on all versions
        .output()
        .await
        .map_err(|e| format!("failed to exec cloudflared tunnel list: {}", e))?;
    if !out.status.success() {
        return Err(format!("tunnel list failed: {}", utils::bytes_to_string(&out.stderr)));
    }
    let stdout = utils::bytes_to_string(&out.stdout);
    for line in stdout.lines() {
        // naive parse: expect the line containing the name also contains a UUID
        if line.contains(name) {
            if let Some(id) = extract_uuid(line) { return Ok(Some(id)); }
        }
    }
    Ok(None)
}

fn extract_uuid(s: &str) -> Option<String> {
    // crude UUID v4 matcher
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

async fn route_dns(name: &str, hostname: &str) -> Result<(), String> {
    let out = Command::new("cloudflared")
        .args(["tunnel", "route", "dns", name, hostname])
        .output()
        .await
        .map_err(|e| format!("failed to exec cloudflared tunnel route dns: {}", e))?;
    if !out.status.success() {
        // If it already exists, proceed
        let err = utils::bytes_to_string(&out.stderr);
        if !err.to_lowercase().contains("already exists") {
            return Err(format!("route dns failed: {}", err));
        }
    }
    Ok(())
}

async fn write_config(uuid: &str, hostname: &str, port: u16) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("cannot resolve home directory")?;
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
    tokio::fs::create_dir_all(&cfg_dir).await.map_err(|e| e.to_string())?;
    tokio::fs::write(&cfg_path, content).await.map_err(|e| e.to_string())?;
    Ok(cfg_path)
}
