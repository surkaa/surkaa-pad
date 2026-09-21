use super::{AiConversationSourceMessage, AiUsage};
use crate::object_locations::MAX_AI_MESSAGE_BLOCK_LEVEL;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use thiserror::Error;

pub const CURRENT_AI_SESSION_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMeta {
    pub version: u32,
    pub id: String,
    pub title: String,
    pub ai_title: Option<String>,
    #[specta(type = f64)]
    pub created_at: i64,
    #[specta(type = f64)]
    pub updated_at: i64,
    // 已经完成消息块写入并由 meta 确认的连续消息数量。
    #[specta(type = f64)]
    pub committed_message_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMessage {
    #[specta(type = f64)]
    pub index: u64,
    #[specta(type = f64)]
    pub created_at: i64,
    pub payload: AiSessionMessagePayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionDetail {
    pub meta: AiSessionMeta,
    pub messages: Vec<AiSessionMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMessagePage {
    pub messages: Vec<AiSessionMessage>,
    #[specta(type = f64)]
    pub total_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMessageBlock {
    pub version: u32,
    pub session_id: String,
    pub level: u32,
    #[specta(type = f64)]
    pub block_id: u64,
    pub messages: Vec<AiSessionMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    tag = "role"
)]
pub enum AiSessionMessagePayload {
    User {
        content: String,
        /// 保存发送消息时的本地时区偏移，使日期变化提示能在不同设备上稳定重建。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[specta(rename = "timezoneOffsetMinutes")]
        timezone_offset_minutes: Option<i16>,
    },
    Assistant {
        state: AiAssistantRecordState,
        content: String,
        error: Option<String>,
        model: String,
        usage: Option<AiUsage>,
        /// 最后一次模型请求实际使用的上下文 Token；旧版 V1 消息没有该字段时保持为空。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[specta(rename = "contextTokens", type = Option<f64>)]
        context_tokens: Option<u64>,
        #[specta(rename = "processSteps")]
        process_steps: Vec<AiProcessStepRecord>,
        /// 本轮新产生的 assistant/tool 消息，不重复保存系统提示和历史轮次。
        trace: Vec<AiConversationSourceMessage>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiAssistantRecordState {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiProcessStepRecord {
    pub id: String,
    pub kind: AiProcessStepKind,
    pub title: String,
    pub detail: Option<String>,
    pub reasoning: String,
    pub state: AiProcessStepState,
    #[specta(type = Option<f64>)]
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiProcessStepKind {
    Model,
    Tool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiProcessStepState {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Error)]
pub enum AiSessionDataError {
    #[error("AI 会话数据不是有效的 JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("AI 会话数据无效: {0}")]
    InvalidData(String),
    #[error("不支持 AI 会话数据版本 V{found}，当前仅支持 V{supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
}

pub fn deserialize_session_meta(
    expected_id: &str,
    bytes: &[u8],
) -> Result<AiSessionMeta, AiSessionDataError> {
    let json = inspect_document(bytes)?;
    validate_current_version(&json)?;
    let meta: AiSessionMeta = serde_json::from_value(json)?;
    if meta.id != expected_id {
        return Err(AiSessionDataError::InvalidData(format!(
            "会话 ID {} 与请求的 ID {expected_id} 不一致",
            meta.id
        )));
    }
    if !is_numeric_id(&meta.id) {
        return Err(AiSessionDataError::InvalidData("会话 ID 必须为数字".into()));
    }
    if meta.title.trim().is_empty() {
        return Err(AiSessionDataError::InvalidData("会话标题不能为空".into()));
    }
    if meta
        .ai_title
        .as_deref()
        .is_some_and(|title| title.trim().is_empty())
    {
        return Err(AiSessionDataError::InvalidData(
            "AI 生成的会话标题不能为空字符串".into(),
        ));
    }
    if meta.updated_at < meta.created_at {
        return Err(AiSessionDataError::InvalidData(
            "会话更新时间不能早于创建时间".into(),
        ));
    }
    Ok(meta)
}

pub fn deserialize_session_message_block(
    expected_session_id: &str,
    expected_level: u32,
    expected_block_id: u64,
    bytes: &[u8],
) -> Result<AiSessionMessageBlock, AiSessionDataError> {
    let json = inspect_document(bytes)?;
    validate_current_version(&json)?;
    let block: AiSessionMessageBlock = serde_json::from_value(json)?;
    if block.session_id != expected_session_id {
        return Err(AiSessionDataError::InvalidData(format!(
            "消息块所属会话 {} 与请求的会话 {expected_session_id} 不一致",
            block.session_id
        )));
    }
    if block.level != expected_level || block.block_id != expected_block_id {
        return Err(AiSessionDataError::InvalidData(format!(
            "消息块位置 {}/{} 与请求的位置 {expected_level}/{expected_block_id} 不一致",
            block.level, block.block_id
        )));
    }
    validate_session_message_block(&block)?;
    Ok(block)
}

pub fn ai_message_block_size(level: u32) -> Option<u64> {
    (level <= MAX_AI_MESSAGE_BLOCK_LEVEL)
        .then(|| 10_u64.checked_pow(level))
        .flatten()
}

/// 按连续版本逐步迁移 AI 会话元数据或消息块；返回 `Some` 时由仓储加密写回。
pub fn migrate_session_document(bytes: &[u8]) -> Result<Option<Vec<u8>>, AiSessionDataError> {
    let mut json = inspect_document(bytes)?;
    let original_version = document_version(&json)?;
    let mut version = original_version;
    while version < CURRENT_AI_SESSION_VERSION {
        match version {
            1 => migrate_v1_to_v2(&mut json)?,
            _ => {
                return Err(AiSessionDataError::UnsupportedVersion {
                    found: original_version,
                    supported: CURRENT_AI_SESSION_VERSION,
                });
            }
        }
        version = version.saturating_add(1);
        json["version"] = Value::Number(version.into());
    }
    if version == original_version {
        Ok(None)
    } else {
        Ok(Some(serde_json::to_vec(&json)?))
    }
}

fn migrate_v1_to_v2(_json: &mut Value) -> Result<(), AiSessionDataError> {
    // V2 从会话元数据中移除了当前模型。具体字段由反序列化为 V2 结构后自然丢弃；
    // 消息块结构没有变化，但仍随会话文档统一升级版本。
    Ok(())
}

fn inspect_document(bytes: &[u8]) -> Result<Value, AiSessionDataError> {
    let json: Value = serde_json::from_slice(bytes)?;
    let version = document_version(&json)?;
    if version > CURRENT_AI_SESSION_VERSION {
        return Err(AiSessionDataError::UnsupportedVersion {
            found: version,
            supported: CURRENT_AI_SESSION_VERSION,
        });
    }
    Ok(json)
}

fn document_version(json: &Value) -> Result<u32, AiSessionDataError> {
    json.get("version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .filter(|version| *version > 0)
        .ok_or_else(|| AiSessionDataError::InvalidData("version 必须是正整数".to_string()))
}

fn validate_current_version(json: &Value) -> Result<(), AiSessionDataError> {
    let version = document_version(json)?;
    if version != CURRENT_AI_SESSION_VERSION {
        return Err(AiSessionDataError::UnsupportedVersion {
            found: version,
            supported: CURRENT_AI_SESSION_VERSION,
        });
    }
    Ok(())
}

fn validate_message_payload(payload: &AiSessionMessagePayload) -> Result<(), AiSessionDataError> {
    match payload {
        AiSessionMessagePayload::User { content, .. } if content.trim().is_empty() => Err(
            AiSessionDataError::InvalidData("用户消息内容不能为空".into()),
        ),
        AiSessionMessagePayload::User {
            timezone_offset_minutes: Some(offset),
            ..
        } if !(-14 * 60..=14 * 60).contains(offset) => Err(AiSessionDataError::InvalidData(
            "用户消息的时区偏移超出有效范围".into(),
        )),
        AiSessionMessagePayload::Assistant {
            state: AiAssistantRecordState::Completed,
            content,
            ..
        } if content.trim().is_empty() => Err(AiSessionDataError::InvalidData(
            "已完成的助手消息内容不能为空".into(),
        )),
        AiSessionMessagePayload::Assistant { model, .. } if model.trim().is_empty() => Err(
            AiSessionDataError::InvalidData("助手消息模型不能为空".into()),
        ),
        _ => Ok(()),
    }
}

pub(crate) fn validate_session_message_block(
    block: &AiSessionMessageBlock,
) -> Result<(), AiSessionDataError> {
    let block_size = ai_message_block_size(block.level).ok_or_else(|| {
        AiSessionDataError::InvalidData(format!("消息块等级 {} 超出范围", block.level))
    })?;
    let expected_len = usize::try_from(block_size)
        .map_err(|_| AiSessionDataError::InvalidData(format!("消息块等级 {} 过大", block.level)))?;
    if block.messages.len() != expected_len {
        return Err(AiSessionDataError::InvalidData(format!(
            "{level} 级消息块必须包含 {block_size} 条消息，实际为 {actual}",
            level = block.level,
            actual = block.messages.len()
        )));
    }
    let start = block
        .block_id
        .checked_mul(block_size)
        .ok_or_else(|| AiSessionDataError::InvalidData("消息块起始索引溢出".into()))?;
    start
        .checked_add(block_size.saturating_sub(1))
        .ok_or_else(|| AiSessionDataError::InvalidData("消息块结束索引溢出".into()))?;
    for (offset, message) in block.messages.iter().enumerate() {
        let offset = u64::try_from(offset)
            .map_err(|_| AiSessionDataError::InvalidData("消息块偏移量溢出".into()))?;
        let expected_index = start
            .checked_add(offset)
            .ok_or_else(|| AiSessionDataError::InvalidData("消息索引溢出".into()))?;
        if message.index != expected_index {
            return Err(AiSessionDataError::InvalidData(format!(
                "消息块内索引不连续：期望 {expected_index}，实际为 {}",
                message.index
            )));
        }
        validate_message_payload(&message.payload)?;
    }
    Ok(())
}

fn is_numeric_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meta_json(version: u32) -> Vec<u8> {
        let mut value = json!({
            "version": version,
            "id": "8215021834823",
            "title": "最近的日记",
            "aiTitle": null,
            "createdAt": 1_700_000_000_000_i64,
            "updatedAt": 1_700_000_000_100_i64,
            "committedMessageCount": 2
        });
        if version == 1 {
            value["model"] = json!("deepseek-chat");
        }
        serde_json::to_vec(&value).unwrap()
    }

    #[test]
    fn deserializes_current_session_meta_and_checks_identity() {
        let meta =
            deserialize_session_meta("8215021834823", &meta_json(CURRENT_AI_SESSION_VERSION))
                .unwrap();
        assert_eq!(meta.title, "最近的日记");
        assert_eq!(meta.committed_message_count, 2);
        let serialized = serde_json::to_value(meta).unwrap();
        assert!(serialized.get("model").is_none());

        assert!(matches!(
            deserialize_session_meta("other", &meta_json(CURRENT_AI_SESSION_VERSION)),
            Err(AiSessionDataError::InvalidData(message)) if message.contains("不一致")
        ));
    }

    #[test]
    fn migrates_v1_and_rejects_newer_or_malformed_versions() {
        let migrated = migrate_session_document(&meta_json(1)).unwrap().unwrap();
        let migrated_json: Value = serde_json::from_slice(&migrated).unwrap();
        assert_eq!(migrated_json["version"], json!(CURRENT_AI_SESSION_VERSION));
        let meta = deserialize_session_meta("8215021834823", &migrated).unwrap();
        assert_eq!(meta.version, CURRENT_AI_SESSION_VERSION);
        assert!(serde_json::to_value(meta).unwrap().get("model").is_none());
        assert!(
            migrate_session_document(&meta_json(CURRENT_AI_SESSION_VERSION))
                .unwrap()
                .is_none()
        );

        assert!(matches!(
            migrate_session_document(&meta_json(CURRENT_AI_SESSION_VERSION + 1)),
            Err(AiSessionDataError::UnsupportedVersion {
                found,
                supported: CURRENT_AI_SESSION_VERSION,
            }) if found == CURRENT_AI_SESSION_VERSION + 1
        ));
        for value in [json!({}), json!({"version": 0}), json!({"version": "1"})] {
            assert!(matches!(
                migrate_session_document(&serde_json::to_vec(&value).unwrap()),
                Err(AiSessionDataError::InvalidData(_))
            ));
        }
    }

    #[test]
    fn roundtrips_level_one_message_block() {
        let mut messages = vec![
            AiSessionMessage {
                index: 0,
                created_at: 1,
                payload: AiSessionMessagePayload::User {
                    content: "总结最近的日记".into(),
                    timezone_offset_minutes: None,
                },
            },
            AiSessionMessage {
                index: 1,
                created_at: 2,
                payload: AiSessionMessagePayload::Assistant {
                    state: AiAssistantRecordState::Completed,
                    content: "这是总结".into(),
                    error: None,
                    model: "deepseek-chat".into(),
                    usage: Some(AiUsage {
                        prompt_tokens: 20,
                        completion_tokens: 5,
                        total_tokens: 25,
                    }),
                    context_tokens: Some(25),
                    process_steps: vec![],
                    trace: vec![AiConversationSourceMessage::Assistant {
                        reasoning_content: Some("需要总结".into()),
                        content: Some("这是总结".into()),
                        tool_calls: vec![],
                    }],
                },
            },
        ];
        for index in 2..10 {
            messages.push(AiSessionMessage {
                index,
                created_at: index as i64,
                payload: AiSessionMessagePayload::User {
                    content: format!("消息 {index}"),
                    timezone_offset_minutes: None,
                },
            });
        }
        let block = AiSessionMessageBlock {
            version: CURRENT_AI_SESSION_VERSION,
            session_id: "8215021834823".into(),
            level: 1,
            block_id: 0,
            messages,
        };
        let bytes = serde_json::to_vec(&block).unwrap();
        assert_eq!(
            deserialize_session_message_block("8215021834823", 1, 0, &bytes).unwrap(),
            block
        );
    }

    #[test]
    fn rejects_invalid_message_identity_and_completed_empty_answer() {
        let value = json!({
            "version": CURRENT_AI_SESSION_VERSION,
            "sessionId": "8215021834823",
            "level": 0,
            "blockId": 3,
            "messages": [{
                "index": 3,
                "createdAt": 1,
                "payload": {
                    "role": "assistant",
                    "state": "completed",
                    "content": " ",
                    "error": null,
                    "model": "model",
                    "usage": null,
                    "processSteps": [],
                    "trace": []
                }
            }]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            deserialize_session_message_block("8215021834823", 0, 3, &bytes),
            Err(AiSessionDataError::InvalidData(message)) if message.contains("不能为空")
        ));
        assert!(matches!(
            deserialize_session_message_block("8215021834823", 0, 4, &bytes),
            Err(AiSessionDataError::InvalidData(message)) if message.contains("位置")
        ));
    }

    #[test]
    fn validates_block_size_and_contiguous_indices() {
        let message = |index| AiSessionMessage {
            index,
            created_at: index as i64,
            payload: AiSessionMessagePayload::User {
                content: format!("消息 {index}"),
                timezone_offset_minutes: None,
            },
        };
        let level_zero = AiSessionMessageBlock {
            version: CURRENT_AI_SESSION_VERSION,
            session_id: "8215021834823".into(),
            level: 0,
            block_id: 20,
            messages: vec![message(20)],
        };
        let bytes = serde_json::to_vec(&level_zero).unwrap();
        assert_eq!(
            deserialize_session_message_block("8215021834823", 0, 20, &bytes).unwrap(),
            level_zero
        );

        let invalid = AiSessionMessageBlock {
            version: CURRENT_AI_SESSION_VERSION,
            session_id: "8215021834823".into(),
            level: 1,
            block_id: 2,
            messages: (20..29).map(message).collect(),
        };
        assert!(matches!(
            deserialize_session_message_block(
                "8215021834823",
                1,
                2,
                &serde_json::to_vec(&invalid).unwrap(),
            ),
            Err(AiSessionDataError::InvalidData(message)) if message.contains("10 条")
        ));

        let mut non_contiguous: Vec<_> = (20..30).map(message).collect();
        non_contiguous[5].index = 99;
        let invalid = AiSessionMessageBlock {
            messages: non_contiguous,
            ..invalid
        };
        assert!(matches!(
            deserialize_session_message_block(
                "8215021834823",
                1,
                2,
                &serde_json::to_vec(&invalid).unwrap(),
            ),
            Err(AiSessionDataError::InvalidData(message)) if message.contains("不连续")
        ));
    }
}
