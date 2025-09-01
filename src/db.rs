use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use sqlx::Row;
use std::str::FromStr;

pub async fn init_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::from_str("sqlite:drcv.db")
                .unwrap()
                .create_if_missing(true)
        ).await
        .expect("sqlite open failed");

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS uploads (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            filename     TEXT NOT NULL,
            size         INTEGER NOT NULL DEFAULT 0,
            status       TEXT NOT NULL,         -- init | uploading | complete | disconnected
            client_ip    TEXT NOT NULL,
            started_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL,
            completed_at TEXT
        )
    "#).execute(&pool).await.expect("migrate failed");

    pool
}

// init: 같은 IP에서 같은 파일명이 있고 완료되지 않았으면 그 id 사용, 없으면 생성
pub async fn init_upload(pool: &SqlitePool, filename: &str, client_ip: &str) -> i64 {
    if let Some(row) = sqlx::query("SELECT id FROM uploads WHERE filename = ?1 AND client_ip = ?2 AND status != 'complete'")
        .bind(filename)
        .bind(client_ip)
        .fetch_optional(pool).await.expect("select failed") {
        return row.try_get::<i64, _>("id").expect("id");
    }

    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"INSERT INTO uploads(filename,size,status,client_ip,started_at,updated_at)
           VALUES(?1, 0, 'init', ?2, ?3, ?3)"#)
        .bind(filename)
        .bind(client_ip)
        .bind(&now)
        .execute(pool).await.expect("insert init failed");

    result.last_insert_rowid()
}

pub async fn mark_uploading(pool: &SqlitePool, id: i64, delta_size: i64) {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"UPDATE uploads
           SET size = size + ?1, status = 'uploading', updated_at = ?2
           WHERE id = ?3"#)
        .bind(delta_size)
        .bind(&now)
        .bind(id)
        .execute(pool).await.expect("update uploading failed");
}

pub async fn mark_complete(pool: &SqlitePool, id: i64) {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"UPDATE uploads
           SET status = 'complete', updated_at = ?1, completed_at = ?1
           WHERE id = ?2"#)
        .bind(&now)
        .bind(id)
        .execute(pool).await.expect("update complete failed");
}

pub async fn mark_stale_uploads_disconnected(pool: &SqlitePool, timeout_minutes: i64) {
    let cutoff_time = chrono::Utc::now() - chrono::Duration::minutes(timeout_minutes);
    let cutoff_str = cutoff_time.to_rfc3339();
    
    // 먼저 disconnected로 마크될 업로드들을 조회
    if let Ok(rows) = sqlx::query(
        r#"SELECT filename, client_ip FROM uploads 
           WHERE status = 'uploading' AND updated_at < ?1"#)
        .bind(&cutoff_str)
        .fetch_all(pool).await {
        
        for row in rows {
            let filename: String = row.get("filename");
            let client_ip: String = row.get("client_ip");
            println!("❌ Upload disconnected (heartbeat timeout): {} from {}", filename, client_ip);
        }
    }
    
    sqlx::query(
        r#"UPDATE uploads
           SET status = 'disconnected', updated_at = ?1
           WHERE status = 'uploading' AND updated_at < ?2"#)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&cutoff_str)
        .execute(pool).await.expect("mark stale uploads failed");
}
