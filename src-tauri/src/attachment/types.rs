use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum AddAttachmentEvent {
    Started,
    /// 0-100 的上传进度百分比
    Progress(u8),
    Completed(AttachmentMeta),
    Error(String),
}

#[derive(Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum DownloadAttachmentEvent {
    Started,
    /// 0-100 的下载进度百分比
    Progress(u8),
    /// 下载完成，保存在应用目录AppData下的路径
    Completed(String),
    Error(String),
}

// 单个附件的元数据
#[derive(Deserialize, Serialize, Clone, Debug, Type)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mimetype: String,
    #[specta(type = f64)]
    pub size: u64,
    #[serde(default)]
    pub encrypted: bool,
    pub nonce: Option<Vec<u8>>, // 用于加密该文件的独立 IV
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::from_str;

    // 测试AttachmentMeta的向下兼容能力
    #[test]
    fn test_attachment_meta_backward_compatibility() {
        let old1 = r#"{
            "filename": "example.txt",
            "mimetype": "text/plain",
            "size": 12345,
            "nonce": [1, 2, 3, 4]
        }"#;
        let old2 = r#"{
            "filename": "example.txt",
            "mimetype": "text/plain",
            "size": 12345
        }"#;

        let att1: AttachmentMeta = from_str(old1).expect("未能将旧的AttachmentMeta反序列化");
        let att2: AttachmentMeta = from_str(old2).expect("未能将旧的AttachmentMeta反序列化");
        assert_eq!(att1.nonce, Some(vec![1, 2, 3, 4]));
        assert!(!att1.encrypted); // 新字段，旧数据中默认为 false
        assert_eq!(att2.nonce, None);
        assert!(!att2.encrypted); // 新字段，旧数据中默认为 false
    }
}
