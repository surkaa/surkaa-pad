use super::{AiConversationSourceMessage, AiUsage};
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
    pub version: u32,
    pub session_id: String,
    #[specta(type = f64)]
    pub index: u64,
    #[specta(type = f64)]
    pub created_at: i64,
    pub payload: AiSessionMessagePayload,
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

pub fn deserialize_session_message(
    expected_session_id: &str,
    expected_index: u64,
    bytes: &[u8],
) -> Result<AiSessionMessage, AiSessionDataError> {
    let json = inspect_document(bytes)?;
    validate_current_version(&json)?;
    let message: AiSessionMessage = serde_json::from_value(json)?;
    if message.session_id != expected_session_id {
        return Err(AiSessionDataError::InvalidData(format!(
            "消息所属会话 {} 与请求的会话 {expected_session_id} 不一致",
            message.session_id
        )));
    }
    if message.index != expected_index {
        return Err(AiSessionDataError::InvalidData(format!(
            "消息索引 {} 与请求的索引 {expected_index} 不一致",
            message.index
        )));
    }
    validate_message_payload(&message.payload)?;
    Ok(message)
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
    fn roundtrips_user_and_assistant_messages() {
        let messages = [
            AiSessionMessage {
                version: CURRENT_AI_SESSION_VERSION,
                session_id: "8215021834823".into(),
                index: 0,
                created_at: 1,
                payload: AiSessionMessagePayload::User {
                    content: "总结最近的日记".into(),
                },
            },
            AiSessionMessage {
                version: CURRENT_AI_SESSION_VERSION,
                session_id: "8215021834823".into(),
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

        for message in messages {
            let bytes = serde_json::to_vec(&message).unwrap();
            assert_eq!(
                deserialize_session_message(&message.session_id, message.index, &bytes).unwrap(),
                message
            );
        }
    }

    #[test]
    fn rejects_invalid_message_identity_and_completed_empty_answer() {
        let value = json!({
            "version": 1,
            "sessionId": "8215021834823",
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
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            deserialize_session_message("8215021834823", 3, &bytes),
            Err(AiSessionDataError::InvalidData(message)) if message.contains("不能为空")
        ));
        assert!(matches!(
            deserialize_session_message("8215021834823", 4, &bytes),
            Err(AiSessionDataError::InvalidData(message)) if message.contains("索引")
        ));
    }
}
