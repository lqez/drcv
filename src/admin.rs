use axum::{extract::{Query, State}, response::{IntoResponse, Sse, sse::Event}, Json};
use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use tokio_stream::StreamExt;
use std::convert::Infallible;

#[derive(Deserialize)]
pub struct ListQuery {
    page: Option<usize>,
    q: Option<String>,
}

#[derive(Serialize)]
pub struct UploadData {
    pub id: i64,
    pub filename: String,
    pub size: i64,
    pub status: String,
    pub client_ip: String,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

pub async fn admin_data(
    State(pool): State<SqlitePool>,
    Query(params): Query<ListQuery>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1).max(1);
    let offset: i64 = ((page - 1) * 100) as i64;
    let q = params.q.unwrap_or_default();

    let rows = if q.is_empty() {
        sqlx::query(
            r#"SELECT id, filename, size, status, started_at, updated_at, completed_at
               FROM uploads ORDER BY id DESC LIMIT 100 OFFSET ?1"#)
            .bind(offset)
            .fetch_all(&pool).await.expect("select failed")
    } else {
        sqlx::query(
            r#"SELECT id, filename, size, status, started_at, updated_at, completed_at
               FROM uploads WHERE filename LIKE ?1
               ORDER BY id DESC LIMIT 100 OFFSET ?2"#)
            .bind(format!("%{}%", q))
            .bind(offset)
            .fetch_all(&pool).await.expect("select failed")
    };

    let out: Vec<serde_json::Value> = rows.into_iter().map(|r| {
        serde_json::json!({
            "id":           r.get::<i64, _>("id"),
            "filename":     r.get::<String, _>("filename"),
            "size":         r.get::<i64, _>("size"),
            "status":       r.get::<String, _>("status"),
            "started_at":   r.get::<String, _>("started_at"),
            "updated_at":   r.get::<String, _>("updated_at"),
            "completed_at": r.try_get::<String, _>("completed_at").ok(),
        })
    }).collect();

    Json(out)
}

pub async fn admin_clients(
    State(pool): State<SqlitePool>,
) -> impl IntoResponse {
    let clients = crate::db::get_connected_clients(&pool).await;
    Json(clients)
}

pub async fn admin_events(
    State(pool): State<SqlitePool>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    use tokio_stream::wrappers::IntervalStream;
    use tokio::time::{interval, Duration};
    use chrono::Utc;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    let last_check = Arc::new(Mutex::new(Utc::now().to_rfc3339()));
    let interval_stream = IntervalStream::new(interval(Duration::from_secs(1)));
    
    let stream = interval_stream.then({
        let last_check = last_check.clone();
        move |_| {
            let pool = pool.clone();
            let last_check = last_check.clone();
            
            async move {
                let current_time = Utc::now().to_rfc3339();
                let mut check_time_guard = last_check.lock().await;
                let check_time = check_time_guard.clone();
                *check_time_guard = current_time;
                drop(check_time_guard);
                
                // 마지막 체크 이후 업데이트된 레코드들 조회
                if let Ok(rows) = sqlx::query(
                    r#"SELECT id, filename, size, status, client_ip, started_at, updated_at, completed_at
                       FROM uploads 
                       WHERE updated_at > ?1 
                       ORDER BY updated_at ASC"#)
                    .bind(&check_time)
                    .fetch_all(&pool).await {
                    
                    if !rows.is_empty() {
                        let updates: Vec<UploadData> = rows.into_iter().map(|row| {
                            UploadData {
                                id: row.get("id"),
                                filename: row.get("filename"),
                                size: row.get("size"),
                                status: row.get("status"),
                                client_ip: row.get("client_ip"),
                                started_at: row.get("started_at"),
                                updated_at: row.get("updated_at"),
                                completed_at: row.try_get("completed_at").ok(),
                            }
                        }).collect();
                        
                        return Ok(Event::default()
                            .event("updates")
                            .data(serde_json::to_string(&updates).unwrap()));
                    }
                }
                
                // 업데이트가 없으면 heartbeat
                Ok(Event::default().data("heartbeat"))
            }
        }
    });

    Sse::new(stream)
}
