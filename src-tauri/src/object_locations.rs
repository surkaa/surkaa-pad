const DIARIES_DIRECTORY: &str = "diaries";
const MANIFEST_FILENAME: &str = "manifest.enc";
const ATTACHMENTS_DIRECTORY: &str = "attachments";
const ATTACHMENT_TRANSACTIONS_DIRECTORY: &str = ".attachment-transaction";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StoredObject {
    DiaryManifest {
        diary_id: String,
    },
    DiaryAttachment {
        diary_id: String,
        attachment_id: String,
    },
    DiaryAttachmentBackup {
        diary_id: String,
        attachment_id: String,
    },
}

impl StoredObject {
    pub fn diary_id(&self) -> &str {
        match self {
            Self::DiaryManifest { diary_id }
            | Self::DiaryAttachment { diary_id, .. }
            | Self::DiaryAttachmentBackup { diary_id, .. } => diary_id,
        }
    }
}

/// 应用当前使用的唯一对象位置定义。
///
/// 业务模块只能通过这里生成或解析 Key，不能自行拼接目录名称。
pub struct ObjectLocations;

impl ObjectLocations {
    pub const fn diaries_prefix() -> &'static str {
        "diaries/"
    }

    pub fn diary_prefix(diary_id: &str) -> String {
        format!("{DIARIES_DIRECTORY}/{diary_id}/")
    }

    pub fn diary_manifest(diary_id: &str) -> String {
        format!("{DIARIES_DIRECTORY}/{diary_id}/{MANIFEST_FILENAME}")
    }

    pub fn diary_attachments_prefix(diary_id: &str) -> String {
        format!("{DIARIES_DIRECTORY}/{diary_id}/{ATTACHMENTS_DIRECTORY}/")
    }

    pub fn diary_attachment(diary_id: &str, attachment_id: &str) -> String {
        format!("{DIARIES_DIRECTORY}/{diary_id}/{ATTACHMENTS_DIRECTORY}/{attachment_id}")
    }

    pub fn diary_attachment_backup(diary_id: &str, attachment_id: &str) -> String {
        format!(
            "{DIARIES_DIRECTORY}/{diary_id}/{ATTACHMENT_TRANSACTIONS_DIRECTORY}/{attachment_id}"
        )
    }

    pub fn key(object: &StoredObject) -> String {
        match object {
            StoredObject::DiaryManifest { diary_id } => Self::diary_manifest(diary_id),
            StoredObject::DiaryAttachment {
                diary_id,
                attachment_id,
            } => Self::diary_attachment(diary_id, attachment_id),
            StoredObject::DiaryAttachmentBackup {
                diary_id,
                attachment_id,
            } => Self::diary_attachment_backup(diary_id, attachment_id),
        }
    }

    pub fn parse(key: &str) -> Option<StoredObject> {
        let mut parts = key.split('/');
        if parts.next()? != DIARIES_DIRECTORY {
            return None;
        }
        let diary_id = valid_diary_id(parts.next()?)?.to_string();
        match (parts.next()?, parts.next(), parts.next()) {
            (MANIFEST_FILENAME, None, None) => Some(StoredObject::DiaryManifest { diary_id }),
            (ATTACHMENTS_DIRECTORY, Some(attachment_id), None) => {
                Some(StoredObject::DiaryAttachment {
                    diary_id,
                    attachment_id: valid_leaf(attachment_id)?.to_string(),
                })
            }
            (ATTACHMENT_TRANSACTIONS_DIRECTORY, Some(attachment_id), None) => {
                Some(StoredObject::DiaryAttachmentBackup {
                    diary_id,
                    attachment_id: valid_leaf(attachment_id)?.to_string(),
                })
            }
            _ => None,
        }
    }

    pub fn diary_id_from_common_prefix(prefix: &str) -> Option<String> {
        let rest = prefix.strip_prefix(Self::diaries_prefix())?;
        let diary_id = rest.strip_suffix('/')?;
        Some(valid_diary_id(diary_id)?.to_string())
    }

    pub fn is_diary_attachment(key: &str) -> bool {
        matches!(Self::parse(key), Some(StoredObject::DiaryAttachment { .. }))
    }

    pub fn is_diary_attachment_for(key: &str, diary_id: &str) -> bool {
        matches!(
            Self::parse(key),
            Some(StoredObject::DiaryAttachment {
                diary_id: object_diary_id,
                ..
            }) if object_diary_id == diary_id
        )
    }
}

/// 一次性正式数据迁移所需的旧 Key 解析器。
///
/// 应用运行逻辑不得使用它读取旧结构。
pub struct LegacyObjectLocations;

impl LegacyObjectLocations {
    pub fn parse(key: &str) -> Option<StoredObject> {
        let mut parts = key.split('/');
        let diary_id = valid_diary_id(parts.next()?)?.to_string();
        match (parts.next()?, parts.next(), parts.next()) {
            (MANIFEST_FILENAME, None, None) => Some(StoredObject::DiaryManifest { diary_id }),
            (ATTACHMENT_TRANSACTIONS_DIRECTORY, Some(attachment_id), None) => {
                Some(StoredObject::DiaryAttachmentBackup {
                    diary_id,
                    attachment_id: valid_leaf(attachment_id)?.to_string(),
                })
            }
            (attachment_id, None, None) => Some(StoredObject::DiaryAttachment {
                diary_id,
                attachment_id: valid_leaf(attachment_id)?.to_string(),
            }),
            _ => None,
        }
    }
}

fn valid_diary_id(value: &str) -> Option<&str> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())).then_some(value)
}

fn valid_leaf(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centralizes_current_diary_object_locations() {
        assert_eq!(ObjectLocations::diaries_prefix(), "diaries/");
        assert_eq!(
            ObjectLocations::diary_prefix("8215021834823"),
            "diaries/8215021834823/"
        );
        assert_eq!(
            ObjectLocations::diary_manifest("8215021834823"),
            "diaries/8215021834823/manifest.enc"
        );
        assert_eq!(
            ObjectLocations::diary_attachments_prefix("8215021834823"),
            "diaries/8215021834823/attachments/"
        );
        assert_eq!(
            ObjectLocations::diary_attachment("8215021834823", "att-1"),
            "diaries/8215021834823/attachments/att-1"
        );
        assert_eq!(
            ObjectLocations::diary_attachment_backup("8215021834823", "att-1"),
            "diaries/8215021834823/.attachment-transaction/att-1"
        );
    }

    #[test]
    fn current_locations_roundtrip_all_known_objects() {
        let objects = [
            StoredObject::DiaryManifest {
                diary_id: "8215021834823".into(),
            },
            StoredObject::DiaryAttachment {
                diary_id: "8215021834823".into(),
                attachment_id: "att-1".into(),
            },
            StoredObject::DiaryAttachmentBackup {
                diary_id: "8215021834823".into(),
                attachment_id: "att-1".into(),
            },
        ];
        for object in objects {
            let key = ObjectLocations::key(&object);
            assert_eq!(ObjectLocations::parse(&key), Some(object));
        }
    }

    #[test]
    fn legacy_locations_map_to_current_object_identities() {
        let cases = [
            (
                "8215021834823/manifest.enc",
                "diaries/8215021834823/manifest.enc",
            ),
            (
                "8215021834823/att-1",
                "diaries/8215021834823/attachments/att-1",
            ),
            (
                "8215021834823/.attachment-transaction/att-1",
                "diaries/8215021834823/.attachment-transaction/att-1",
            ),
        ];
        for (legacy_key, current_key) in cases {
            let object = LegacyObjectLocations::parse(legacy_key).unwrap();
            assert_eq!(ObjectLocations::key(&object), current_key);
        }
    }

    #[test]
    fn rejects_unknown_or_malformed_locations() {
        for key in [
            "8215021834823/manifest.enc",
            "diaries/not-a-number/manifest.enc",
            "diaries/8215021834823/attachments/",
            "diaries/8215021834823/attachments/att-1/extra",
            "ai/session/meta.enc",
        ] {
            assert_eq!(ObjectLocations::parse(key), None, "key={key}");
        }
        for key in [
            "not-a-number/manifest.enc",
            "8215021834823/dir/attachment",
            "rust-tests/run/8215021834823/manifest.enc",
            "diaries/8215021834823/manifest.enc",
        ] {
            assert_eq!(LegacyObjectLocations::parse(key), None, "key={key}");
        }
    }

    #[test]
    fn parses_only_current_diary_common_prefixes() {
        assert_eq!(
            ObjectLocations::diary_id_from_common_prefix("diaries/8215021834823/"),
            Some("8215021834823".into())
        );
        for prefix in [
            "8215021834823/",
            "diaries/not-a-number/",
            "diaries/8215021834823/attachments/",
            "ai/",
        ] {
            assert_eq!(
                ObjectLocations::diary_id_from_common_prefix(prefix),
                None,
                "prefix={prefix}"
            );
        }
    }

    #[test]
    fn recognizes_only_current_attachment_locations() {
        assert!(ObjectLocations::is_diary_attachment_for(
            "diaries/8215021834823/attachments/att-1",
            "8215021834823"
        ));
        for key in [
            "8215021834823/att-1",
            "diaries/8215021834823/manifest.enc",
            "diaries/8215021834823/.attachment-transaction/att-1",
        ] {
            assert!(!ObjectLocations::is_diary_attachment(key), "key={key}");
        }
    }
}
