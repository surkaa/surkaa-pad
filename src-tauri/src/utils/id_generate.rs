use std::sync::atomic::{AtomicI64, Ordering};

static LAST_ISSUED_TIMESTAMP: AtomicI64 = AtomicI64::new(0);

pub fn generate_descending_id() -> u64 {
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

pub fn generate_descending_id_with_timestamp(timestamp: i64) -> u64 {
    9_999_999_999_999u64.saturating_sub(timestamp.max(0) as u64)
}

/// 将前端传入的 f64（JS number）校验并转换为日记 ID。
/// 当前 ID 是 13 位以内的反向时间戳，f64 可精确表示，无需担心精度丢失。
pub fn checked_u64_from_f64(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
        return None;
    }
    u64::try_from(value as u128).ok()
}
