const DIARIES_DIRECTORY: &str = "diaries";
const MANIFEST_FILENAME: &str = "manifest.enc";
const ATTACHMENTS_DIRECTORY: &str = "attachments";
const ATTACHMENT_TRANSACTIONS_DIRECTORY: &str = ".attachment-transaction";
const AI_DIRECTORY: &str = "ai";
const AI_SESSIONS_DIRECTORY: &str = "sessions";
const AI_SESSION_META_FILENAME: &str = "meta.enc";
const AI_SESSION_MESSAGES_DIRECTORY: &str = "messages";
/// `u64` 消息索引最多只需要 0–19 级十进制块。
/// 极高等级的块理论上可能触及对象存储约 5 GB 的单对象上限，但真实会话几乎不可能
/// 累积到对应消息数量，现阶段不为这个不可达场景引入按字节再次拆块的复杂度。
pub const MAX_AI_MESSAGE_BLOCK_LEVEL: u32 = 19;

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
    AiSessionMeta {
        session_id: String,
    },
    AiSessionMessageBlock {
        session_id: String,
        level: u32,
        block_id: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredObjectCollection {
    AiSessionMetas,
    AiSessionMessageBlocks { session_id: String },
}

impl StoredObjectCollection {
    pub fn contains(&self, object: &StoredObject) -> bool {
        match (self, object) {
            (Self::AiSessionMetas, StoredObject::AiSessionMeta { .. }) => true,
            (
                Self::AiSessionMessageBlocks { session_id },
                StoredObject::AiSessionMessageBlock {
                    session_id: object_session_id,
                    ..
                },
            ) => session_id == object_session_id,
            _ => false,
        }
    }
}

impl StoredObject {
    pub fn diary_id(&self) -> Option<&str> {
        match self {
            Self::DiaryManifest { diary_id }
            | Self::DiaryAttachment { diary_id, .. }
            | Self::DiaryAttachmentBackup { diary_id, .. } => Some(diary_id),
            Self::AiSessionMeta { .. } | Self::AiSessionMessageBlock { .. } => None,
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

    pub const fn ai_sessions_prefix() -> &'static str {
        "ai/sessions/"
    }

    pub fn ai_session_prefix(session_id: &str) -> String {
        format!("{AI_DIRECTORY}/{AI_SESSIONS_DIRECTORY}/{session_id}/")
    }

    pub fn ai_session_meta(session_id: &str) -> String {
        format!("{AI_DIRECTORY}/{AI_SESSIONS_DIRECTORY}/{session_id}/{AI_SESSION_META_FILENAME}")
    }

    pub fn ai_session_messages_prefix(session_id: &str) -> String {
        format!(
            "{AI_DIRECTORY}/{AI_SESSIONS_DIRECTORY}/{session_id}/{AI_SESSION_MESSAGES_DIRECTORY}/"
        )
    }

    pub fn ai_session_message_level_prefix(session_id: &str, level: u32) -> String {
        format!(
            "{AI_DIRECTORY}/{AI_SESSIONS_DIRECTORY}/{session_id}/{AI_SESSION_MESSAGES_DIRECTORY}/{level}/"
        )
    }

    pub fn ai_session_message_block(session_id: &str, level: u32, block_id: u64) -> String {
        format!(
            "{AI_DIRECTORY}/{AI_SESSIONS_DIRECTORY}/{session_id}/{AI_SESSION_MESSAGES_DIRECTORY}/{level}/{block_id}.enc"
        )
    }

    pub fn ai_session_id_from_common_prefix(prefix: &str) -> Option<String> {
        let rest = prefix.strip_prefix(Self::ai_sessions_prefix())?;
        let session_id = rest.strip_suffix('/')?;
        Some(valid_numeric_id(session_id)?.to_string())
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
            StoredObject::AiSessionMeta { session_id } => Self::ai_session_meta(session_id),
            StoredObject::AiSessionMessageBlock {
                session_id,
                level,
                block_id,
            } => Self::ai_session_message_block(session_id, *level, *block_id),
        }
    }

    pub fn parse(key: &str) -> Option<StoredObject> {
        if key.starts_with(Self::ai_sessions_prefix()) {
            return Self::parse_ai_session(key);
        }
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

    fn parse_ai_session(key: &str) -> Option<StoredObject> {
        let mut parts = key.split('/');
        if parts.next()? != AI_DIRECTORY || parts.next()? != AI_SESSIONS_DIRECTORY {
            return None;
        }
        let session_id = valid_numeric_id(parts.next()?)?.to_string();
        match (parts.next()?, parts.next(), parts.next(), parts.next()) {
            (AI_SESSION_META_FILENAME, None, None, None) => {
                Some(StoredObject::AiSessionMeta { session_id })
            }
            (AI_SESSION_MESSAGES_DIRECTORY, Some(level), Some(filename), None) => {
                let level = parse_ai_message_block_level(level)?;
                let block_id = parse_ai_message_block_id(filename)?;
                Some(StoredObject::AiSessionMessageBlock {
                    session_id,
                    level,
                    block_id,
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
    valid_numeric_id(value)
}

fn valid_numeric_id(value: &str) -> Option<&str> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())).then_some(value)
}

fn parse_ai_message_block_level(value: &str) -> Option<u32> {
    let level = parse_canonical_nonnegative_integer(value)?;
    u32::try_from(level)
        .ok()
        .filter(|level| *level <= MAX_AI_MESSAGE_BLOCK_LEVEL)
}

fn parse_ai_message_block_id(filename: &str) -> Option<u64> {
    parse_canonical_nonnegative_integer(filename.strip_suffix(".enc")?)
}

fn parse_canonical_nonnegative_integer(value: &str) -> Option<u64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value.parse().ok()
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
        assert_eq!(ObjectLocations::ai_sessions_prefix(), "ai/sessions/");
        assert_eq!(
            ObjectLocations::ai_session_meta("8215021834823"),
            "ai/sessions/8215021834823/meta.enc"
        );
        assert_eq!(
            ObjectLocations::ai_session_message_block("8215021834823", 1, 2),
            "ai/sessions/8215021834823/messages/1/2.enc"
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
            StoredObject::AiSessionMeta {
                session_id: "8215021834823".into(),
            },
            StoredObject::AiSessionMessageBlock {
                session_id: "8215021834823".into(),
                level: 1,
                block_id: 2,
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
            "ai/sessions/not-a-number/meta.enc",
            "ai/sessions/8215021834823/messages/12_user.enc",
            "ai/sessions/8215021834823/messages/20/0.enc",
            "ai/sessions/8215021834823/messages/01/0.enc",
            "ai/sessions/8215021834823/messages/1/02.enc",
            "ai/sessions/8215021834823/messages/1/not-a-number.enc",
            "ai/sessions/8215021834823/messages/1/2.enc/extra",
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

    #[test]
    fn filters_ai_object_collections_without_exposing_key_rules() {
        let meta = StoredObject::AiSessionMeta {
            session_id: "1".into(),
        };
        let first_session_block = StoredObject::AiSessionMessageBlock {
            session_id: "1".into(),
            level: 0,
            block_id: 0,
        };
        let other_session_block = StoredObject::AiSessionMessageBlock {
            session_id: "2".into(),
            level: 0,
            block_id: 0,
        };

        assert!(StoredObjectCollection::AiSessionMetas.contains(&meta));
        let blocks = StoredObjectCollection::AiSessionMessageBlocks {
            session_id: "1".into(),
        };
        assert!(blocks.contains(&first_session_block));
        assert!(!blocks.contains(&meta));
        assert!(!blocks.contains(&other_session_block));
        assert_eq!(
            ObjectLocations::ai_session_id_from_common_prefix("ai/sessions/1/"),
            Some("1".into())
        );
        assert_eq!(
            ObjectLocations::ai_session_id_from_common_prefix("ai/sessions/nope/"),
            None
        );
    }
}
