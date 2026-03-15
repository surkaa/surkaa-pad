/// 日记主文件在云存储中的路径
pub fn remote_manifest_key(diary_id: &str) -> String {
    format!("{}/manifest.enc", diary_id)
}

/// 将日记主文件的key解析出日记ID
pub fn diary_id_from_manifest_key(key: &str) -> Option<String> {
    key.strip_suffix("/manifest.enc")
        .map(|stripped| stripped.to_string())
}

/// 日记附件在云存储中的路径
pub fn remote_attachments_key(diary_id: &str, attachment_filename: &str) -> String {
    format!("{}/{}", diary_id, attachment_filename)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn test_path_buf_join_multiple() {
        let root = PathBuf::from("root");
        let path = root
            .join("aaa/bbb/aaaa.txt")
            .to_str()
            .expect("Failed to convert PathBuf to str")
            .replace("\\", "/");
        assert_eq!(&path, "root/aaa/bbb/aaaa.txt");
    }
}
