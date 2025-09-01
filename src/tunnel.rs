use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;
use std::net::{SocketAddrV4, Ipv4Addr};
use tokio::time::sleep;

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterRequest {
    pub port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterResponse {
    pub success: bool,
    pub subdomain: Option<String>,
    pub external_ip: Option<String>,
    pub message: Option<String>,
    pub expires_in: Option<u64>,
}

pub struct TunnelClient {
    tunnel_server: String,
    local_port: u16,
    subdomain: Option<String>,
    external_ip: Option<String>,
    expires_at: Option<std::time::SystemTime>,
}

impl TunnelClient {
    pub fn new(local_port: u16, tunnel_server: String) -> Self {
        // HTTP API 서버로 변경 (WebSocket 대신)
        let api_server = tunnel_server.replace("wss://", "https://").replace("ws://", "http://");
        Self {
            tunnel_server: api_server,
            local_port,
            subdomain: None,
            external_ip: None,
            expires_at: None,
        }
    }
    
    pub async fn register(&mut self) -> Result<String, Box<dyn Error + Send + Sync>> {
        println!("🔧 Setting up UPnP port forwarding...");
        
        // UPnP 포트 포워딩 시도 (옵션)
        match self.setup_port_forwarding().await {
            Ok(_) => println!("✅ UPnP port forwarding successful"),
            Err(e) => {
                println!("⚠️  UPnP port forwarding failed: {}", e);
                println!("💡 Manual port forwarding may be required for external access");
            }
        }
        
        println!("📡 Registering with tunnel server...");
        
        // 터널 서버에 등록 (서버에서 IP 감지)
        let (subdomain, external_ip) = self.register_with_retry().await?;
        
        // 성공 시 정보 저장
        self.subdomain = Some(subdomain.clone());
        self.external_ip = Some(external_ip.clone());
        self.expires_at = Some(std::time::SystemTime::now() + Duration::from_secs(86400)); // 24시간
        
        println!("🌐 External IP detected by server: {}:{}", external_ip, self.local_port);
        println!("🎉 Tunnel established: https://{}.drcv.app", subdomain);
        println!("📁 Share this URL for others to upload files to your computer!");
        println!("⏰ Tunnel expires in 24 hours");
        
        // NAT 환경에서의 주의사항 안내
        if self.is_likely_behind_nat(&external_ip) {
            println!();
            println!("⚠️  NAT Environment Detected:");
            println!("   If running multiple drcv instances behind the same router:");
            println!("   • Use different ports (--upload-port) for each instance");
            println!("   • Ensure port forwarding is configured correctly");
            println!("   • Each instance will get a unique subdomain");
        }
        
        Ok(subdomain)
    }
    
    async fn register_with_retry(&self) -> Result<(String, String), Box<dyn Error + Send + Sync>> {
        let request = RegisterRequest {
            port: self.local_port,
        };
        
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        
        for attempt in 1..=3 {
            match self.try_register(&client, &request).await {
                Ok((subdomain, ip)) => return Ok((subdomain, ip)),
                Err(e) if attempt < 3 => {
                    println!("⚠️  Registration attempt {} failed: {}", attempt, e);
                    println!("🔄 Retrying in 2 seconds...");
                    sleep(Duration::from_secs(2)).await;
                }
                Err(e) => return Err(e),
            }
        }
        
        unreachable!()
    }
    
    async fn try_register(&self, client: &reqwest::Client, request: &RegisterRequest) -> Result<(String, String), Box<dyn Error + Send + Sync>> {
        let response = client
            .post(&format!("{}/register", self.tunnel_server))
            .json(request)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()).into());
        }
        
        let register_response: RegisterResponse = response.json().await?;
        
        if register_response.success {
            if let (Some(subdomain), Some(external_ip)) = (register_response.subdomain, register_response.external_ip) {
                return Ok((subdomain, external_ip));
            }
        }
        
        Err(format!("Registration failed: {}", 
            register_response.message.unwrap_or("Unknown error".to_string())).into())
    }
    
    
    
    async fn setup_port_forwarding(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!("🔧 Attempting UPnP port forwarding...");
        
        // UPnP IGD를 사용한 실제 포트 포워딩
        match tokio::task::spawn_blocking({
            let port = self.local_port;
            move || -> Result<(), Box<dyn Error + Send + Sync>> {
                use igd::*;
                
                // IGD 게이트웨이 찾기
                let gateway = search_gateway(SearchOptions::default())?;
                println!("🌐 Found UPnP gateway: {}", gateway.addr);
                
                // 포트 매핑 추가
                gateway.add_port(
                    PortMappingProtocol::TCP,
                    port,
                    SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port),
                    86400, // 24시간 후 만료
                    "DRCV Upload Server"
                )?;
                
                println!("✅ UPnP port forwarding setup: {} -> {}", port, port);
                Ok(())
            }
        }).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(format!("UPnP setup failed: {}", e).into()),
            Err(e) => Err(format!("UPnP task failed: {}", e).into()),
        }
    }
    
    pub async fn start_keepalive(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let tunnel_server = self.tunnel_server.clone();
        let subdomain = self.subdomain.clone();
        
        if subdomain.is_none() {
            return Err("No active tunnel to keep alive".into());
        }
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5분마다
            
            loop {
                interval.tick().await;
                
                // Health check 요청
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build();
                
                if let Ok(client) = client {
                    if let Ok(response) = client
                        .get(&format!("{}/health", tunnel_server))
                        .send()
                        .await {
                        
                        if response.status().is_success() {
                            println!("💓 Tunnel keepalive: OK");
                        } else {
                            println!("⚠️  Tunnel keepalive: Failed ({})", response.status());
                        }
                    } else {
                        println!("⚠️  Tunnel keepalive: Network error");
                    }
                }
            }
        });
        
        Ok(())
    }
    
    pub fn print_status(&self) {
        println!("🔗 Tunnel Status:");
        
        if let Some(subdomain) = &self.subdomain {
            println!("  📍 URL: https://{}.drcv.app", subdomain);
            
            if let Some(expires_at) = self.expires_at {
                if let Ok(remaining) = expires_at.duration_since(std::time::SystemTime::now()) {
                    let hours = remaining.as_secs() / 3600;
                    let minutes = (remaining.as_secs() % 3600) / 60;
                    println!("  ⏰ Expires in: {}h {}m", hours, minutes);
                } else {
                    println!("  ❌ Expired");
                }
            }
            
            println!("  🌐 Local port: {}", self.local_port);
            println!("  📡 Server: {}", self.tunnel_server);
        } else {
            println!("  ❌ No active tunnel");
        }
    }
    
    pub fn get_external_ip(&self) -> Option<String> {
        self.external_ip.clone()
    }
    
    fn is_likely_behind_nat(&self, external_ip: &str) -> bool {
        // 간단한 NAT 감지 (완전하지는 않음)
        // 실제로는 로컬 IP와 외부 IP를 비교해야 함
        !external_ip.starts_with("127.") && 
        !external_ip.starts_with("192.168.") && 
        !external_ip.starts_with("10.") && 
        !external_ip.starts_with("172.")
    }
}