/// 日记主文件在云存储中的路径
pub fn remote_manifest_key(diary_id: &str) -> String {
    format!("{}/manifest.enc", diary_id)
}

/// 将日记主文件的key解析出日记ID
pub fn diary_id_from_manifest_key(key: &str) -> Option<String> {
    key.strip_suffix("/manifest.enc").map(|stripped| stripped.to_string())
}

/// 日记附件在云存储中的路径
pub fn remote_attachments_key(diary_id: &str, attachment_filename: &str) -> String {
    format!("{}/{}", diary_id, attachment_filename)
}
