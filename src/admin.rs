use axum::{extract::{Query, State}, response::IntoResponse, Json};
use serde::Deserialize;
use sqlx::SqlitePool;
use sqlx::Row;

#[derive(Deserialize)]
pub struct ListQuery {
    page: Option<usize>,
    q: Option<String>,
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
