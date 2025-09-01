use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use sqlx::Row;

pub async fn init_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:./drcv.db").await
        .expect("sqlite open failed");

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS uploads (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            filename     TEXT NOT NULL,
            size         INTEGER NOT NULL DEFAULT 0,
            status       TEXT NOT NULL,         -- init | uploading | complete
            started_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL,
            completed_at TEXT
        )
    "#).execute(&pool).await.expect("migrate failed");

    pool
}

// init: 존재하면 그 id 사용, 없으면 생성
pub async fn init_upload(pool: &SqlitePool, filename: &str) -> i64 {
    if let Some(row) = sqlx::query("SELECT id FROM uploads WHERE filename = ?1 AND status != 'complete'")
        .bind(filename)
        .fetch_optional(pool).await.expect("select failed") {
        return row.try_get::<i64, _>("id").expect("id");
    }

    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"INSERT INTO uploads(filename,size,status,started_at,updated_at)
           VALUES(?1, 0, 'init', ?2, ?2)"#)
        .bind(filename)
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
