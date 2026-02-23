use crate::object::ByteStream;
use futures_util::StreamExt;

/// 包装一个 ByteStream 来跟踪上传进度
/// # Arguments
/// * `len` - 总字节数
/// * `stream` - 原始的 ByteStream
/// * `progress_update` - 一个回调函数，进度更新时调用，最多调用100次，参数是当前进度百分比（0-100）
pub fn tracker_stream<F>(len: u64, stream: ByteStream, progress_update: F) -> ByteStream
where
    F: Fn(u8) + Send + 'static,
{
    // 记录累计已上传的字节数
    let mut uploaded = 0u64;
    // 记录上一次触发回调的百分比
    let mut last_percent = 0u8;

    Box::pin(stream.inspect(move |result| {
        if let Ok(bytes) = result {
            uploaded += bytes.len() as u64;

            // 计算当前百分比
            let current_percent = if len != 0 {
                std::cmp::min((uploaded as u128 * 100 / len as u128) as u8, 100)
            } else {
                100
            };

            // 只有当整数百分比发生跃升时，才调用回调
            if current_percent > last_percent {
                progress_update(current_percent);
                last_percent = current_percent;
            }
        }
    }))
}
