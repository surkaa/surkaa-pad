use crate::storage::types::PathGetter;
use std::path::PathBuf;

/// 日记主文件在云存储中的路径
pub fn remote_manifest_key(diary_id: &str) -> String {
    format!("{}/manifest.enc", diary_id)
}

/// 将日记主文件的key解析出日记ID
pub fn diary_id_from_manifest_key(key: &str) -> Option<String> {
    if let Some(stripped) = key.strip_suffix("/manifest.enc") {
        Some(stripped.to_string())
    } else {
        None
    }
}

pub fn is_remote_manifest_key(key: &str) -> bool {
    key.ends_with("/manifest.enc")
}

/// 日记附件在云存储中的路径
pub fn remote_attachments_key(diary_id: &str, attachment_filename: &str) -> String {
    format!("{}/{}.enc", diary_id, attachment_filename)
}

/// 日记主文件在本地缓存中的路径
pub fn local_manifest_path(
    path_getter: &impl PathGetter,
    diary_id: &str,
    hash_or_etag: &str,
) -> PathBuf {
    path_getter
        .get_data_path()
        .join("diaries")
        .join(format!("{}_{}.enc", diary_id, hash_or_etag))
}

/// 日记附件在本地临时目录的路径
pub fn local_attachment_path(
    path_getter: &impl PathGetter,
    diary_id: &str,
    attachment_filename: &str,
) -> PathBuf {
    path_getter
        .get_data_path()
        .join("pad")
        .join(format!("{}_{}.raw", diary_id, attachment_filename))
}

/// 日记附件在本地临时目录的文件夹路径
pub fn local_attachment_dir(path_getter: &impl PathGetter) -> PathBuf {
    path_getter.get_data_path().join("pad")
}

/// 获取录音文件夹
pub fn local_recording_dir(path_getter: &impl PathGetter) -> PathBuf {
    path_getter.get_data_path().join("audio_cache")
}
