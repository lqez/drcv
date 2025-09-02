use axum::{extract::{State, ConnectInfo, Query, Extension}, response::IntoResponse, http::{HeaderMap, StatusCode}, Json};
use axum_typed_multipart::{TryFromMultipart, TypedMultipart, FieldData};
use sqlx::{SqlitePool, Row};
use std::{fs, path::PathBuf, net::SocketAddr, collections::HashMap};
use tokio::io::AsyncWriteExt;
use serde::Deserialize;
use crate::{db, AppConfig};

fn extract_client_ip(headers: &HeaderMap, addr: &SocketAddr) -> String {
    let peer_ip = addr.ip();
    // Only trust proxy headers when the peer is a trusted proxy (loopback = cloudflared local)
    let trust_headers = peer_ip.is_loopback();

    if trust_headers {
        // 1) CF-Connecting-IP (Cloudflare)
        if let Some(v) = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()) {
            let v = v.trim();
            if !v.is_empty() { return v.to_string(); }
        }
        // 2) True-Client-IP (some proxies)
        if let Some(v) = headers.get("true-client-ip").and_then(|v| v.to_str().ok()) {
            let v = v.trim();
            if !v.is_empty() { return v.to_string(); }
        }
        // 3) X-Forwarded-For: take the left-most entry
        if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            let first = v.split(',').next().map(|s| s.trim()).unwrap_or("");
            if !first.is_empty() { return first.to_string(); }
        }
        // 4) X-Real-IP
        if let Some(v) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            let v = v.trim();
            if !v.is_empty() { return v.to_string(); }
        }
    }

    // Fallback to the direct peer address
    peer_ip.to_string()
}

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    pub upload_ids: Vec<i64>,
}

#[derive(TryFromMultipart)]
pub struct ChunkUploadRequest {
    pub filename: String,
    pub chunk_index: u32,
    pub total_chunks: u32,
    #[form_data(limit = "8GiB")]
    pub chunk: FieldData<bytes::Bytes>,
}

pub async fn handle_chunk_upload(
    State(pool): State<SqlitePool>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(config): Extension<AppConfig>,
    headers: HeaderMap,
    TypedMultipart(upload_data): TypedMultipart<ChunkUploadRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // 즉시 클라이언트 heartbeat 업데이트(표시에 지연 없도록)
    let client_ip = extract_client_ip(&headers, &addr);
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok());
    db::update_client_heartbeat(&pool, &client_ip, user_agent).await;
    // 요청 처리 중 연결 끊어짐 감지를 위한 future 생성
    let client_ip_clone = client_ip.clone();
    let upload_future = process_chunk_upload(pool.clone(), config, upload_data, client_ip_clone);
    
    // 연결 끊어짐이나 타임아웃 처리
    match tokio::time::timeout(std::time::Duration::from_secs(300), upload_future).await {
        Ok(result) => result,
        Err(_) => {
            // 타임아웃 발생 (클라이언트 연결 끊어짐 가능성)
            println!("⚠️ Upload timeout - client may have disconnected");
            Err((StatusCode::REQUEST_TIMEOUT, "Upload timeout".to_string()))
        }
    }
}

async fn process_chunk_upload(
    pool: SqlitePool,
    config: AppConfig,
    upload_data: ChunkUploadRequest,
    client_ip: String,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let save_dir = &config.upload_dir;
    fs::create_dir_all(save_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create directory: {}", e)))?;

    // 기존 업로드 정보 확인 (db::init_upload 호출 전에)
    let existing_upload = sqlx::query("SELECT id, size FROM uploads WHERE filename = ?1 AND client_ip = ?2 AND status != 'complete'")
        .bind(&upload_data.filename)
        .bind(&client_ip)
        .fetch_optional(&pool).await
        .expect("select failed");
    
    let id = db::init_upload(&pool, &upload_data.filename, &client_ip).await;
    
    // 총 파일 크기 계산 및 체크
    let estimated_file_size = (upload_data.chunk.contents.len() as u64) * (upload_data.total_chunks as u64);
    if estimated_file_size > config.max_file_size {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("File too large: {} bytes exceeds limit of {} bytes", estimated_file_size, config.max_file_size)));
    }
    
    let tmp_path = PathBuf::from(save_dir).join(format!("{}.part", upload_data.filename));
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&tmp_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to open file: {}", e)))?;

    // 업로드 세션의 첫 번째 청크인지 확인 (static으로 추적)
    use std::sync::Mutex;
    use std::collections::HashSet;
    static LOGGED_UPLOADS: once_cell::sync::Lazy<Mutex<HashSet<i64>>> = once_cell::sync::Lazy::new(|| Mutex::new(HashSet::new()));
    
    {
        let mut logged = LOGGED_UPLOADS.lock().unwrap();
        if !logged.contains(&id) {
            logged.insert(id);
            
            if let Some(row) = existing_upload {
                let existing_size: i64 = row.get("size");
                if existing_size > 0 {
                    println!("🔄 Resuming upload: {} (from {} bytes, chunk {})", upload_data.filename, existing_size, upload_data.chunk_index);
                } else {
                    println!("▶️ Starting upload: {}", upload_data.filename);
                }
            } else {
                println!("▶️ Starting upload: {}", upload_data.filename);
            }
        }
    }

    let chunk_data = &upload_data.chunk.contents;
    if !chunk_data.is_empty() {
        file.write_all(chunk_data)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write chunk: {}", e)))?;
        db::mark_uploading(&pool, id, chunk_data.len() as i64).await;
    }

    if upload_data.chunk_index + 1 == upload_data.total_chunks {
        let final_path = PathBuf::from(save_dir).join(&upload_data.filename);
        tokio::fs::rename(&tmp_path, &final_path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to finalize file: {}", e)))?;
        println!("✅ Completed upload: {:?}", final_path);
        db::mark_complete(&pool, id).await;
    }

    Ok(id.to_string())
}

pub async fn handle_upload_head(
    State(pool): State<SqlitePool>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let filename = params.get("filename").unwrap_or(&"".to_string()).clone();
    let client_ip = extract_client_ip(&headers, &addr);
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok());
    // 업로드 초기 HEAD 시점에도 즉시 클라이언트 표시
    db::update_client_heartbeat(&pool, &client_ip, user_agent).await;
    
    // 같은 IP에서 진행 중인 업로드가 있는지 확인
    if let Ok(Some(row)) = sqlx::query("SELECT size FROM uploads WHERE filename = ?1 AND client_ip = ?2 AND status != 'complete'")
        .bind(&filename)
        .bind(&client_ip)
        .fetch_optional(&pool).await {
        
        let uploaded_bytes: i64 = row.try_get("size").unwrap_or(0);
        let mut headers = HeaderMap::new();
        headers.insert("x-uploaded-bytes", uploaded_bytes.to_string().parse().unwrap());
        return (headers, "").into_response();
    }
    
    // 진행 중인 업로드가 없으면 0바이트
    let mut headers = HeaderMap::new();
    headers.insert("x-uploaded-bytes", "0".parse().unwrap());
    (headers, "").into_response()
}

pub async fn handle_heartbeat(
    State(pool): State<SqlitePool>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<HeartbeatRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let client_ip = extract_client_ip(&headers, &addr);
    let now = chrono::Utc::now().to_rfc3339();
    
    // User-Agent 헤더 추출
    let user_agent = headers.get("user-agent")
        .and_then(|v| v.to_str().ok());
    
    // 클라이언트 heartbeat 업데이트
    db::update_client_heartbeat(&pool, &client_ip, user_agent).await;
    
    let mut updated_count = 0;
    
    for upload_id in request.upload_ids {
        // 각 ID의 업로드가 같은 IP에서 진행 중인지 확인하고 updated_at 갱신
        match sqlx::query(
            r#"UPDATE uploads 
               SET updated_at = ?1 
               WHERE id = ?2 AND client_ip = ?3 AND status = 'uploading'"#)
            .bind(&now)
            .bind(upload_id)
            .bind(&client_ip)
            .execute(&pool).await {
            
            Ok(result) => {
                updated_count += result.rows_affected();
            },
            Err(e) => {
                println!("Heartbeat error for upload {}: {}", upload_id, e);
            }
        }
    }
    
    Ok(format!("heartbeat_ok:{}", updated_count))
}
