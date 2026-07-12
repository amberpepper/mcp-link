use chrono::Utc;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}
