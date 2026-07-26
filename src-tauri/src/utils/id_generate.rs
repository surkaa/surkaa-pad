use std::sync::atomic::{AtomicI64, Ordering};

static LAST_ISSUED_TIMESTAMP: AtomicI64 = AtomicI64::new(0);

pub fn generate_descending_id() -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let mut last = LAST_ISSUED_TIMESTAMP.load(Ordering::Relaxed);
    let timestamp = loop {
        let candidate = now.max(last.saturating_add(1));
        match LAST_ISSUED_TIMESTAMP.compare_exchange_weak(
            last,
            candidate,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break candidate,
            Err(current) => last = current,
        }
    };
    generate_descending_id_with_timestamp(timestamp)
}

pub fn generate_descending_id_with_timestamp(timestamp: i64) -> String {
    format!("{:013}", 9999999999999 - timestamp) // 13个9的毫秒级时间戳对应 2286-11-21 01:46:39
}
