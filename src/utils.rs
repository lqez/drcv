use chrono::Utc;

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn bytes_to_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}