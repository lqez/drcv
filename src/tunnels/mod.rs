use async_trait::async_trait;
use std::fmt;

#[derive(Debug)]
pub enum TunnelError {
    NotInstalled(String),
    ConfigError(String),
    NetworkError(String),
    AuthError(String),
}

impl fmt::Display for TunnelError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TunnelError::NotInstalled(msg) => write!(f, "Tunnel not installed: {}", msg),
            TunnelError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            TunnelError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            TunnelError::AuthError(msg) => write!(f, "Authentication error: {}", msg),
        }
    }
}

impl std::error::Error for TunnelError {}

pub struct TunnelConfig {
    pub hostname_root: String,
    pub local_port: u16,
}

#[async_trait]
pub trait TunnelProvider: Send + Sync {
    async fn ensure(&self, db: &sqlx::SqlitePool, config: &TunnelConfig) -> Result<Box<dyn TunnelManager>, TunnelError>;
}

#[async_trait]
pub trait TunnelManager: Send + Sync {
    fn hostname(&self) -> &str;
    async fn run(&self) -> Result<Box<dyn TunnelRunner>, TunnelError>;
}

#[async_trait]
pub trait TunnelRunner: Send + Sync {
    async fn shutdown(self: Box<Self>) -> Result<(), TunnelError>;
}

pub mod cloudflare;

pub use cloudflare::CloudflareTunnelProvider;

pub fn create_tunnel_provider(provider_name: &str) -> Result<Box<dyn TunnelProvider>, TunnelError> {
    match provider_name.to_lowercase().as_str() {
        "cloudflare" => Ok(Box::new(CloudflareTunnelProvider)),
        _ => Err(TunnelError::ConfigError(format!("Unknown tunnel provider: {}", provider_name)))
    }
}