use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use sqlx::Row;
use std::str::FromStr;
use log::{error, warn};
use crate::utils;

pub async fn init_pool() -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::from_str("sqlite:drcv.db")
                .unwrap()
                .create_if_missing(true)
        ).await?;

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
    "#).execute(&pool).await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_uploads_updated_at ON uploads(updated_at)")
        .execute(&pool).await?;

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS clients (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            client_ip    TEXT NOT NULL UNIQUE,
            user_agent   TEXT,
            first_seen   TEXT NOT NULL,
            last_seen    TEXT NOT NULL,
            status       TEXT NOT NULL DEFAULT 'connected'  -- connected | disconnected
        )
    "#).execute(&pool).await?;

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS kv (
            k TEXT PRIMARY KEY,
            v TEXT NOT NULL
        )
    "#).execute(&pool).await?;

    Ok(pool)
}

pub async fn kv_get(pool: &SqlitePool, key: &str) -> Option<String> {
    if let Ok(row) = sqlx::query("SELECT v FROM kv WHERE k = ?1")
        .bind(key)
        .fetch_optional(pool).await {
        return row.and_then(|r| r.try_get::<String, _>("v").ok());
    }
    None
}

pub async fn kv_set(pool: &SqlitePool, key: &str, value: &str) {
    let _ = sqlx::query(
        r#"INSERT INTO kv(k, v) VALUES(?1, ?2)
           ON CONFLICT(k) DO UPDATE SET v = excluded.v"#)
        .bind(key)
        .bind(value)
        .execute(pool).await;
}

pub async fn init_upload(pool: &SqlitePool, filename: &str, client_ip: &str) -> i64 {
    match sqlx::query("SELECT id FROM uploads WHERE filename = ?1 AND client_ip = ?2 AND status != 'complete'")
        .bind(filename)
        .bind(client_ip)
        .fetch_optional(pool).await {
        Ok(Some(row)) => {
            return row.try_get::<i64, _>("id").unwrap_or_else(|e| {
                error!("Error getting upload id: {}", e);
                0
            });
        },
        Ok(None) => {},
        Err(e) => {
            error!("Database error in init_upload: {}", e);
            return 0;
        }
    }

    let now = utils::now();
    match sqlx::query(
        r#"INSERT INTO uploads(filename,size,status,client_ip,started_at,updated_at)
           VALUES(?1, 0, 'init', ?2, ?3, ?3)"#)
        .bind(filename)
        .bind(client_ip)
        .bind(&now)
        .execute(pool).await {
        Ok(result) => result.last_insert_rowid(),
        Err(e) => {
            error!("Failed to insert upload: {}", e);
            0
        }
    }
}

pub async fn mark_uploading(pool: &SqlitePool, id: i64, delta_size: i64) {
    let now = utils::now();
    sqlx::query(
        r#"UPDATE uploads
           SET size = size + ?1, status = 'uploading', updated_at = ?2
           WHERE id = ?3"#)
        .bind(delta_size)
        .bind(&now)
        .bind(id)
        .execute(pool).await.map_err(|e| {
            error!("Failed to update upload status: {}", e);
            e
        }).ok();
}

pub async fn mark_complete(pool: &SqlitePool, id: i64) {
    let now = utils::now();
    sqlx::query(
        r#"UPDATE uploads
           SET status = 'complete', updated_at = ?1, completed_at = ?1
           WHERE id = ?2"#)
        .bind(&now)
        .bind(id)
        .execute(pool).await.map_err(|e| {
            error!("Failed to mark upload complete: {}", e);
            e
        }).ok();
}

pub async fn update_client_heartbeat(pool: &SqlitePool, client_ip: &str, user_agent: Option<&str>) {
    let now = utils::now();
    
    sqlx::query(
        r#"INSERT INTO clients (client_ip, user_agent, first_seen, last_seen, status)
           VALUES (?1, ?2, ?3, ?3, 'connected')
           ON CONFLICT(client_ip) DO UPDATE SET
           user_agent = COALESCE(?2, user_agent),
           last_seen = ?3,
           status = 'connected'"#)
        .bind(client_ip)
        .bind(user_agent)
        .bind(&now)
        .execute(pool).await.map_err(|e| {
            error!("Failed to update client heartbeat: {}", e);
            e
        }).ok();
}

pub async fn get_connected_clients(pool: &SqlitePool) -> Vec<serde_json::Value> {
    if let Ok(rows) = sqlx::query(
        r#"SELECT client_ip, user_agent, first_seen, last_seen, status
           FROM clients 
           WHERE status = 'connected'
           ORDER BY last_seen DESC"#)
        .fetch_all(pool).await {
        
        rows.into_iter().map(|row| {
            serde_json::json!({
                "client_ip": row.get::<String, _>("client_ip"),
                "user_agent": row.try_get::<String, _>("user_agent").ok(),
                "first_seen": row.get::<String, _>("first_seen"),
                "last_seen": row.get::<String, _>("last_seen"),
                "status": row.get::<String, _>("status")
            })
        }).collect()
    } else {
        Vec::new()
    }
}

pub async fn mark_stale_uploads_disconnected(pool: &SqlitePool, timeout_seconds: i64) {
    let cutoff_time = chrono::Utc::now() - chrono::Duration::seconds(timeout_seconds);
    let cutoff_str = cutoff_time.to_rfc3339();
    
    if let Ok(rows) = sqlx::query(
        r#"SELECT filename, client_ip FROM uploads 
           WHERE status = 'uploading' AND updated_at < ?1"#)
        .bind(&cutoff_str)
        .fetch_all(pool).await {
        
        for row in rows {
            let filename: String = row.get("filename");
            let client_ip: String = row.get("client_ip");
            warn!("❌ Upload disconnected (heartbeat timeout): {} from {}", filename, client_ip);
        }
    }
    
    sqlx::query(
        r#"UPDATE uploads
           SET status = 'disconnected', updated_at = ?1
           WHERE status = 'uploading' AND updated_at < ?2"#)
        .bind(utils::now())
        .bind(&cutoff_str)
        .execute(pool).await.map_err(|e| {
            error!("Failed to mark stale uploads: {}", e);
            e
        }).ok();
}

pub async fn mark_stale_clients_disconnected(pool: &SqlitePool, timeout_seconds: i64) {
    let cutoff_time = chrono::Utc::now() - chrono::Duration::seconds(timeout_seconds);
    let cutoff_str = cutoff_time.to_rfc3339();
    
    sqlx::query(
        r#"DELETE FROM clients
           WHERE status = 'connected' AND last_seen < ?1"#)
        .bind(&cutoff_str)
        .execute(pool).await.map_err(|e| {
            error!("Failed to delete stale clients: {}", e);
            e
        }).ok();
}
