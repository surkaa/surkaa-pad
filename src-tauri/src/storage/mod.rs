mod types;

pub use self::types::CachePathGetter;
use std::path::PathBuf;

/// 日记主文件在云存储中的路径
pub fn remote_manifest_key(diary_id: &str) -> String {
    format!("{}/manifest.enc", diary_id)
}

pub fn is_remote_manifest_key(key: &str) -> bool {
    key.ends_with("/manifest.enc")
}

/// 日记附件在云存储中的路径
pub fn remote_attachments_key(diary_id: &str, attachment_id: &str) -> String {
    format!("{}/{}.enc", diary_id, attachment_id)
}

/// 日记主文件在本地缓存中的路径
pub fn local_manifest_path(
    cache_path: impl CachePathGetter,
    diary_id: &str,
    hash_or_etag: &str,
) -> PathBuf {
    cache_path
        .get_cache_path()
        .join("diaries")
        .join(format!("{}_{}.enc", diary_id, hash_or_etag))
}

/// 日记附件在本地临时目录的路径
pub fn local_attachment_path(
    cache_path: impl CachePathGetter,
    diary_id: &str,
    attachment_id: &str,
) -> PathBuf {
    cache_path
        .get_temp_path()
        .join("pad")
        .join(format!("{}_{}.raw", diary_id, attachment_id))
}
