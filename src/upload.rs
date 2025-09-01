use axum::{extract::{Multipart, State}, response::IntoResponse};
use sqlx::SqlitePool;
use std::{fs, path::PathBuf};
use tokio::io::AsyncWriteExt;
use crate::db;

pub async fn handle_chunk_upload(
    State(pool): State<SqlitePool>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let save_dir = "./uploads";
    fs::create_dir_all(save_dir).unwrap();

    let mut filename = String::new();
    let mut chunk_index: usize = 0;
    let mut total_chunks: usize = 0;
    let mut data: Vec<u8> = Vec::new();

    while let Some(field) = multipart.next_field().await.unwrap() {
        match field.name() {
            Some("filename") => {
                filename = String::from_utf8(field.bytes().await.unwrap().to_vec()).unwrap();
            }
            Some("chunk_index") => {
                chunk_index = String::from_utf8(field.bytes().await.unwrap().to_vec()).unwrap().parse().unwrap();
            }
            Some("total_chunks") => {
                total_chunks = String::from_utf8(field.bytes().await.unwrap().to_vec()).unwrap().parse().unwrap();
            }
            Some("chunk") => {
                data = field.bytes().await.unwrap().to_vec();
            }
            _ => {}
        }
    }

    let id = db::init_upload(&pool, &filename).await;           // (1) init (중복 init 방지)
    let tmp_path = PathBuf::from(save_dir).join(format!("{}.part", filename));
    let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(&tmp_path).await.unwrap();

    if chunk_index == 0 {
        println!("▶️ Starting upload: {}", filename);
    }
    if !data.is_empty() {
        file.write_all(&data).await.unwrap();
        db::mark_uploading(&pool, id, data.len() as i64).await; // (2) uploading + size 누적
    }

    if chunk_index + 1 == total_chunks {
        let final_path = PathBuf::from(save_dir).join(&filename);
        tokio::fs::rename(&tmp_path, &final_path).await.unwrap();
        println!("✅ Completed upload: {:?}", final_path);
        db::mark_complete(&pool, id).await;                     // (3) complete
    }

    "OK"
}
