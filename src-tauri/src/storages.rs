/// 日记主文件在云存储中的路径
pub fn remote_manifest_key(diary_id: &str) -> String {
    format!("{}/manifest.enc", diary_id)
}

/// 将日记主文件的key解析出日记ID
pub fn diary_id_from_manifest_key(key: &str) -> Option<String> {
    let diary_id = key.strip_suffix("/manifest.enc")?;
    if diary_id.is_empty() || !diary_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(diary_id.to_string())
}

/// 将对象存储返回的一级目录前缀解析为日记 ID。
pub fn diary_id_from_common_prefix(prefix: &str) -> Option<String> {
    let diary_id = prefix.strip_suffix('/')?;
    if diary_id.is_empty() || !diary_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(diary_id.to_string())
}

/// 日记附件在云存储中的路径
pub fn remote_attachments_key(diary_id: &str, attachment_filename: &str) -> String {
    format!("{}/{}", diary_id, attachment_filename)
}

/// 判断 key 是否为当前日记存储结构中的一级附件对象。
///
/// 只接受 `<数字日记 ID>/<附件文件名>`，排除 Manifest 和事务备份等内部对象。
pub fn is_diary_attachment_key(key: &str) -> bool {
    let mut parts = key.split('/');
    let Some(diary_id) = parts.next() else {
        return false;
    };
    let Some(filename) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !diary_id.is_empty()
        && diary_id.bytes().all(|byte| byte.is_ascii_digit())
        && !filename.is_empty()
        && filename != "manifest.enc"
}

#[cfg(test)]
mod tests {
    use super::{diary_id_from_common_prefix, diary_id_from_manifest_key, is_diary_attachment_key};
    use std::path::PathBuf;

    #[test]
    fn parses_top_level_numeric_manifest_key() {
        assert_eq!(
            diary_id_from_manifest_key("8215021834823/manifest.enc"),
            Some("8215021834823".to_string())
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
    fn parses_only_top_level_numeric_common_prefixes() {
        assert_eq!(
            diary_id_from_common_prefix("8215021834823/"),
            Some("8215021834823".to_string())
        );
        for prefix in [
            "8215021834823",
            "abc/",
            "diaries/8215021834823/",
            "rust-tests/",
            "/",
        ] {
            assert_eq!(diary_id_from_common_prefix(prefix), None, "prefix={prefix}");
        }
    }

    #[test]
    fn recognizes_only_top_level_numeric_diary_attachments() {
        assert!(is_diary_attachment_key(
            "8215021834823/att-9e66f2d29a25c611ba34b2dabfbd5c19"
        ));
        for key in [
            "8215021834823/manifest.enc",
            "8215021834823/.attachment-transaction/att-1",
            "rust-tests/run/8215021834823/att-1",
            "abc/att-1",
            "8215021834823/",
        ] {
            assert!(!is_diary_attachment_key(key), "key={key}");
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
