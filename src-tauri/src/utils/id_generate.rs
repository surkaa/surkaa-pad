pub fn generate_descending_id() -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    generate_descending_id_with_timestamp(timestamp)
}

pub fn generate_descending_id_with_timestamp(timestamp: i64) -> String {
    format!("{:013}", 9999999999999 - timestamp) // 13个9的毫秒级时间戳对应 2286-11-21 01:46:39
}
