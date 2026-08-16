use super::{AiConversationSourceMessage, AiUsage};
use crate::object_locations::MAX_AI_MESSAGE_BLOCK_LEVEL;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use thiserror::Error;

pub const CURRENT_AI_SESSION_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMeta {
    pub version: u32,
    pub id: String,
    pub title: String,
    pub ai_title: Option<String>,
    pub model: String,
    #[specta(type = f64)]
    pub created_at: i64,
    #[specta(type = f64)]
    pub updated_at: i64,
    #[specta(type = f64)]
    pub message_count: u64,
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
    },
    Assistant {
        state: AiAssistantRecordState,
        content: String,
        error: Option<String>,
        model: String,
        usage: Option<AiUsage>,
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
    if meta.model.trim().is_empty() {
        return Err(AiSessionDataError::InvalidData("会话模型不能为空".into()));
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
    validate_message_block(&block)?;
    Ok(block)
}

pub fn ai_message_block_size(level: u32) -> Option<u64> {
    (level <= MAX_AI_MESSAGE_BLOCK_LEVEL)
        .then(|| 10_u64.checked_pow(level))
        .flatten()
}

/// 当前仅有 V1，因此现阶段迁移只验证版本；未来 V2 在这里串接连续 JSON 迁移步骤。
pub fn migrate_session_document(bytes: &[u8]) -> Result<Option<Vec<u8>>, AiSessionDataError> {
    let json = inspect_document(bytes)?;
    validate_current_version(&json)?;
    Ok(None)
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
        AiSessionMessagePayload::User { content } if content.trim().is_empty() => Err(
            AiSessionDataError::InvalidData("用户消息内容不能为空".into()),
        ),
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

fn validate_message_block(block: &AiSessionMessageBlock) -> Result<(), AiSessionDataError> {
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
        serde_json::to_vec(&json!({
            "version": version,
            "id": "8215021834823",
            "title": "最近的日记",
            "aiTitle": null,
            "model": "deepseek-chat",
            "createdAt": 1_700_000_000_000_i64,
            "updatedAt": 1_700_000_000_100_i64,
            "messageCount": 2
        }))
        .unwrap()
    }

    #[test]
    fn deserializes_current_session_meta_and_checks_identity() {
        let meta = deserialize_session_meta("8215021834823", &meta_json(1)).unwrap();
        assert_eq!(meta.title, "最近的日记");
        assert_eq!(meta.message_count, 2);

        assert!(matches!(
            deserialize_session_meta("other", &meta_json(1)),
            Err(AiSessionDataError::InvalidData(message)) if message.contains("不一致")
        ));
    }

    #[test]
    fn rejects_legacy_newer_and_malformed_versions() {
        assert!(matches!(
            migrate_session_document(&meta_json(2)),
            Err(AiSessionDataError::UnsupportedVersion {
                found: 2,
                supported: CURRENT_AI_SESSION_VERSION,
            })
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
            "version": 1,
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
            },
        };
        let level_zero = AiSessionMessageBlock {
            version: 1,
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
            version: 1,
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
