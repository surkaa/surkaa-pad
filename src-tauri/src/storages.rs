/// 日记主文件在云存储中的路径
pub fn remote_manifest_key(diary_id: u64) -> String {
    format!("{diary_id}/manifest.enc")
}

/// 将日记主文件的key解析出日记ID
pub fn diary_id_from_manifest_key(key: &str) -> Option<u64> {
    let diary_id = key.strip_suffix("/manifest.enc")?;
    if diary_id.is_empty() || !diary_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    diary_id.parse().ok()
}

/// 日记附件在云存储中的路径
pub fn remote_attachments_key(diary_id: u64, attachment_filename: &str) -> String {
    format!("{diary_id}/{attachment_filename}")
}

#[cfg(test)]
mod tests {
    use super::diary_id_from_manifest_key;
    use std::path::PathBuf;

    #[test]
    fn parses_top_level_numeric_manifest_key() {
        assert_eq!(
            diary_id_from_manifest_key("8215021834823/manifest.enc"),
            Some(8_215_021_834_823)
        );
    }

    #[test]
    fn rejects_non_diary_manifest_keys() {
        for key in [
            "rust-tests/test-name/run/8215021834823/manifest.enc",
            "abc/manifest.enc",
            "8215021834823/attachments/manifest.enc",
            "/manifest.enc",
            "8215021834823/manifest.json",
            "manifest.enc",
        ] {
            assert_eq!(diary_id_from_manifest_key(key), None, "key={key}");
        }
    }

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
