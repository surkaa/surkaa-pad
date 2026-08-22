use super::{
    AiAgent, AiAgentEvent, AiAgentResponse, AiAssistantMessage, AiAssistantRecordState,
    AiConversationSource, AiConversationSourceMessage, AiError, AiMessage, AiModelProvider,
    AiProcessStepKind, AiProcessStepRecord, AiProcessStepState, AiSessionMessage,
    AiSessionMessagePayload, AiSessionRepository, AiSessionRepositoryError, AiToolDefinition,
    AiToolExecutor,
};
use chrono::{FixedOffset, Local, TimeZone, Utc};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;
use tauri_plugin_log::log;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const INTERRUPTED_ANSWER_ERROR: &str = "上次回答因应用中断未完成";

#[derive(Debug, Error)]
pub(crate) enum AiSessionAgentError {
    #[error(transparent)]
    Repository(#[from] AiSessionRepositoryError),
    #[error("AI 会话消息顺序无效: {0}")]
    InvalidHistory(String),
}

pub(crate) enum AiSessionAgentOutcome {
    Completed {
        response: AiAgentResponse,
        source: AiConversationSource,
    },
    Failed(String),
    Cancelled,
}

pub(crate) struct AiSessionAgentRunner<'a> {
    repository: &'a AiSessionRepository,
    provider: &'a dyn AiModelProvider,
    tools: &'a dyn AiToolExecutor,
}

impl<'a> AiSessionAgentRunner<'a> {
    pub(crate) fn new(
        repository: &'a AiSessionRepository,
        provider: &'a dyn AiModelProvider,
        tools: &'a dyn AiToolExecutor,
    ) -> Self {
        Self {
            repository,
            provider,
            tools,
        }
    }

    /// 自动提交一轮完整问答。用户消息先落盘；无论模型完成、失败还是被取消，
    /// 都会再写入一条助手消息，使持久化历史始终保持用户/助手成对。
    pub(crate) async fn run<F>(
        &self,
        session_id: &str,
        prompt: &str,
        cancellation: CancellationToken,
        emit: &F,
    ) -> Result<AiSessionAgentOutcome, AiSessionAgentError>
    where
        F: Fn(AiAgentEvent) -> Result<(), AiError> + Send + Sync,
    {
        let run_started_at = Instant::now();
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Err(AiSessionRepositoryError::InvalidInput("问题不能为空".into()).into());
        }

        let Some((meta, mut stored_messages)) = self.repository.load_session(session_id).await?
        else {
            return Err(AiSessionRepositoryError::SessionNotFound(session_id.to_owned()).into());
        };
        recover_interrupted_turn(
            self.repository,
            &meta.model,
            session_id,
            &mut stored_messages,
        )
        .await?;
        let model = meta.model;

        let user_save_started_at = Instant::now();
        let user_message = self
            .repository
            .append_message(
                session_id,
                now_millis(),
                AiSessionMessagePayload::User {
                    content: prompt.to_owned(),
                    timezone_offset_minutes: current_timezone_offset_minutes(),
                },
            )
            .await?;
        let user_save_ms = user_save_started_at.elapsed().as_millis();
        stored_messages.push(user_message);
        let history_started_at = Instant::now();
        let history = conversation_history(&stored_messages, true)?;
        let history_prepare_ms = history_started_at.elapsed().as_millis();
        let history_turns = history.completed_turns;

        let recorder = Mutex::new(AiProcessRecorder::default());
        let recorded_emit = |event: AiAgentEvent| {
            recorder
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .record(&event);
            emit(event)
        };
        enum Terminal {
            Completed(super::AiAgentRunResult),
            Failed(AiError),
            Cancelled,
        }
        // 将模型 Future 限定在该作用域；取消分支返回后立即释放网络请求和工具调用资源，
        // 再执行助手消息持久化。
        let agent_started_at = Instant::now();
        let terminal = {
            let agent = AiAgent::new(self.provider, self.tools);
            let run = agent.run_stream_with_message_history_source(
                &model,
                &history.messages,
                prompt,
                &recorded_emit,
            );
            tokio::pin!(run);
            tokio::select! {
                biased;
                result = &mut run => match result {
                    Ok(result) => Terminal::Completed(result),
                    Err(error) => Terminal::Failed(error),
                },
                _ = cancellation.cancelled() => Terminal::Cancelled,
            }
        };
        let agent_ms = agent_started_at.elapsed().as_millis();

        match terminal {
            Terminal::Completed(result) => {
                let process_steps = finish_recorder(&recorder, AiProcessStepState::Completed);
                let assistant_save_started_at = Instant::now();
                self.repository
                    .append_message(
                        session_id,
                        now_millis(),
                        AiSessionMessagePayload::Assistant {
                            state: AiAssistantRecordState::Completed,
                            content: result.response.answer.clone(),
                            error: None,
                            model: model.clone(),
                            usage: result.response.usage,
                            process_steps,
                            trace: current_turn_trace(&result.source),
                        },
                    )
                    .await?;
                log_run_timing(
                    session_id,
                    "completed",
                    history_turns,
                    AiSessionRunTiming {
                        history_prepare_ms,
                        user_save_ms,
                        agent_ms,
                        assistant_save_ms: assistant_save_started_at.elapsed().as_millis(),
                        total_ms: run_started_at.elapsed().as_millis(),
                    },
                );
                Ok(AiSessionAgentOutcome::Completed {
                    response: result.response,
                    source: result.source,
                })
            }
            Terminal::Failed(error) => {
                let message = error.to_string();
                let (process_steps, partial_answer) =
                    finish_recorder_with_answer(&recorder, AiProcessStepState::Failed);
                let assistant_save_started_at = Instant::now();
                self.repository
                    .append_message(
                        session_id,
                        now_millis(),
                        AiSessionMessagePayload::Assistant {
                            state: AiAssistantRecordState::Failed,
                            content: partial_answer,
                            error: Some(message.clone()),
                            model: model.clone(),
                            usage: None,
                            process_steps,
                            trace: vec![],
                        },
                    )
                    .await?;
                log_run_timing(
                    session_id,
                    "failed",
                    history_turns,
                    AiSessionRunTiming {
                        history_prepare_ms,
                        user_save_ms,
                        agent_ms,
                        assistant_save_ms: assistant_save_started_at.elapsed().as_millis(),
                        total_ms: run_started_at.elapsed().as_millis(),
                    },
                );
                Ok(AiSessionAgentOutcome::Failed(message))
            }
            Terminal::Cancelled => {
                let (process_steps, partial_answer) =
                    finish_recorder_with_answer(&recorder, AiProcessStepState::Cancelled);
                let assistant_save_started_at = Instant::now();
                self.repository
                    .append_message(
                        session_id,
                        now_millis(),
                        AiSessionMessagePayload::Assistant {
                            state: AiAssistantRecordState::Cancelled,
                            content: partial_answer,
                            error: None,
                            model: model.clone(),
                            usage: None,
                            process_steps,
                            trace: vec![],
                        },
                    )
                    .await?;
                log_run_timing(
                    session_id,
                    "cancelled",
                    history_turns,
                    AiSessionRunTiming {
                        history_prepare_ms,
                        user_save_ms,
                        agent_ms,
                        assistant_save_ms: assistant_save_started_at.elapsed().as_millis(),
                        total_ms: run_started_at.elapsed().as_millis(),
                    },
                );
                Ok(AiSessionAgentOutcome::Cancelled)
            }
        }
    }
}

struct AiSessionRunTiming {
    history_prepare_ms: u128,
    user_save_ms: u128,
    agent_ms: u128,
    assistant_save_ms: u128,
    total_ms: u128,
}

fn log_run_timing(
    session_id: &str,
    outcome: &str,
    history_turns: usize,
    timing: AiSessionRunTiming,
) {
    log::info!(
        "[ai session timing] operation=run, session_id={}, outcome={}, history_turns={}, history_prepare_ms={}, user_save_ms={}, agent_ms={}, assistant_save_ms={}, total_ms={}",
        session_id,
        outcome,
        history_turns,
        timing.history_prepare_ms,
        timing.user_save_ms,
        timing.agent_ms,
        timing.assistant_save_ms,
        timing.total_ms
    );
}

async fn recover_interrupted_turn(
    repository: &AiSessionRepository,
    model: &str,
    session_id: &str,
    messages: &mut Vec<AiSessionMessage>,
) -> Result<(), AiSessionAgentError> {
    validate_message_order(messages, true)?;
    if messages.len().is_multiple_of(2) {
        return Ok(());
    }

    let recovered = repository
        .append_message(
            session_id,
            now_millis(),
            AiSessionMessagePayload::Assistant {
                state: AiAssistantRecordState::Failed,
                content: String::new(),
                error: Some(INTERRUPTED_ANSWER_ERROR.into()),
                model: model.to_owned(),
                usage: None,
                process_steps: vec![],
                trace: vec![],
            },
        )
        .await?;
    messages.push(recovered);
    Ok(())
}

struct PreparedConversationHistory {
    messages: Vec<AiMessage>,
    completed_turns: usize,
}

fn conversation_history(
    messages: &[AiSessionMessage],
    allow_trailing_user: bool,
) -> Result<PreparedConversationHistory, AiSessionAgentError> {
    validate_message_order(messages, allow_trailing_user)?;
    let mut history = Vec::new();
    let mut completed_turns = 0;
    let mut previous_message_at = None;

    for pair in messages.chunks_exact(2) {
        let AiSessionMessagePayload::User {
            content: user,
            timezone_offset_minutes,
        } = &pair[0].payload
        else {
            unreachable!("message order was validated")
        };
        let AiSessionMessagePayload::Assistant {
            state,
            content,
            trace,
            ..
        } = &pair[1].payload
        else {
            unreachable!("message order was validated")
        };
        if *state != AiAssistantRecordState::Completed {
            continue;
        }

        append_time_context_if_needed(
            &mut history,
            previous_message_at,
            pair[0].created_at,
            *timezone_offset_minutes,
        );
        history.push(AiMessage::User(user.clone()));
        match restore_completed_trace(trace, content) {
            Ok(trace_messages) => history.extend(trace_messages),
            Err(error) => {
                log::warn!(
                    "AI 历史工具轨迹无效，回退到最终回答: message_index={}, error={}",
                    pair[1].index,
                    error
                );
                history.push(final_answer_message(content));
            }
        }
        completed_turns += 1;
        previous_message_at = Some(pair[1].created_at);
    }

    if allow_trailing_user && !messages.len().is_multiple_of(2) {
        let current = messages.last().expect("odd message count has a tail");
        let AiSessionMessagePayload::User {
            timezone_offset_minutes,
            ..
        } = &current.payload
        else {
            unreachable!("message order was validated")
        };
        append_time_context_if_needed(
            &mut history,
            previous_message_at,
            current.created_at,
            *timezone_offset_minutes,
        );
    }

    Ok(PreparedConversationHistory {
        messages: history,
        completed_turns,
    })
}

pub(crate) fn persisted_conversation_source(
    model: &str,
    messages: &[AiSessionMessage],
    tools: &[AiToolDefinition],
) -> Result<AiConversationSource, AiSessionAgentError> {
    let complete_len = messages.len() - messages.len() % 2;
    let history = conversation_history(&messages[..complete_len], false)?;
    Ok(super::agent::conversation_source_for_history(
        model,
        &history.messages,
        tools,
    ))
}

fn append_time_context_if_needed(
    history: &mut Vec<AiMessage>,
    previous_message_at: Option<i64>,
    user_message_at: i64,
    timezone_offset_minutes: Option<i16>,
) {
    let offset_minutes = timezone_offset_minutes.unwrap_or(0);
    let Some(offset) = FixedOffset::east_opt(i32::from(offset_minutes) * 60) else {
        return;
    };
    let Some(current) = offset.timestamp_millis_opt(user_message_at).single() else {
        return;
    };
    let date_changed = previous_message_at
        .and_then(|timestamp| offset.timestamp_millis_opt(timestamp).single())
        .is_none_or(|previous| previous.date_naive() != current.date_naive());
    if !date_changed {
        return;
    }

    let offset_seconds = offset.local_minus_utc();
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let offset_minutes = offset_seconds.unsigned_abs() / 60;
    history.push(AiMessage::System(format!(
        "当前本地日期和时间：{}（UTC{sign}{:02}:{:02}）。",
        current.format("%Y-%m-%d %H:%M"),
        offset_minutes / 60,
        offset_minutes % 60,
    )));
}

fn restore_completed_trace(
    trace: &[AiConversationSourceMessage],
    expected_answer: &str,
) -> Result<Vec<AiMessage>, String> {
    if trace.is_empty() {
        return Ok(vec![final_answer_message(expected_answer)]);
    }

    let mut messages = Vec::with_capacity(trace.len());
    let mut pending_tool_calls = HashSet::new();
    let mut seen_tool_calls = HashSet::new();
    for (position, source_message) in trace.iter().enumerate() {
        match source_message {
            AiConversationSourceMessage::System { .. }
            | AiConversationSourceMessage::User { .. } => {
                return Err("单轮轨迹中不应包含 system 或 user 消息".into());
            }
            AiConversationSourceMessage::Assistant { tool_calls, .. } => {
                if !pending_tool_calls.is_empty() {
                    return Err("新的助手消息出现前仍有工具调用缺少结果".into());
                }
                for call in tool_calls {
                    if !seen_tool_calls.insert(call.id.as_str()) {
                        return Err(format!("工具调用 ID 重复: {}", call.id));
                    }
                    pending_tool_calls.insert(call.id.as_str());
                }
                if tool_calls.is_empty() && position + 1 != trace.len() {
                    return Err("最终回答之后仍存在其他轨迹消息".into());
                }
            }
            AiConversationSourceMessage::Tool { tool_call_id, .. } => {
                if !pending_tool_calls.remove(tool_call_id.as_str()) {
                    return Err(format!("工具结果无法匹配调用 ID: {tool_call_id}"));
                }
            }
        }
        messages.push(AiMessage::try_from(source_message).map_err(|error| error.to_string())?);
    }
    if !pending_tool_calls.is_empty() {
        return Err("会话轨迹末尾仍有工具调用缺少结果".into());
    }
    match messages.last() {
        Some(AiMessage::Assistant(AiAssistantMessage {
            content: Some(content),
            tool_calls,
            ..
        })) if tool_calls.is_empty() && content == expected_answer => Ok(messages),
        _ => Err("轨迹中的最终回答与持久化助手消息不一致".into()),
    }
}

fn final_answer_message(content: &str) -> AiMessage {
    AiMessage::Assistant(AiAssistantMessage {
        reasoning_content: None,
        content: Some(content.to_owned()),
        tool_calls: vec![],
    })
}

fn validate_message_order(
    messages: &[AiSessionMessage],
    allow_trailing_user: bool,
) -> Result<(), AiSessionAgentError> {
    for (position, message) in messages.iter().enumerate() {
        let valid = if position % 2 == 0 {
            matches!(message.payload, AiSessionMessagePayload::User { .. })
        } else {
            matches!(message.payload, AiSessionMessagePayload::Assistant { .. })
        };
        if !valid {
            return Err(AiSessionAgentError::InvalidHistory(format!(
                "索引 {} 的消息角色与预期不符",
                message.index
            )));
        }
    }
    if !allow_trailing_user && !messages.len().is_multiple_of(2) {
        return Err(AiSessionAgentError::InvalidHistory(
            "会话末尾缺少助手消息".into(),
        ));
    }
    Ok(())
}

fn current_turn_trace(source: &AiConversationSource) -> Vec<AiConversationSourceMessage> {
    let Some(user_position) = source
        .messages
        .iter()
        .rposition(|message| matches!(message, AiConversationSourceMessage::User { .. }))
    else {
        return vec![];
    };
    source.messages[user_position + 1..].to_vec()
}

fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

fn current_timezone_offset_minutes() -> Option<i16> {
    let minutes = Local::now().offset().local_minus_utc() / 60;
    i16::try_from(minutes).ok()
}

#[derive(Default)]
struct AiProcessRecorder {
    steps: Vec<PendingProcessStep>,
    answer: String,
}

struct PendingProcessStep {
    id: String,
    kind: AiProcessStepKind,
    title: String,
    detail: Option<String>,
    reasoning: String,
    state: Option<AiProcessStepState>,
    duration_ms: Option<u64>,
}

impl AiProcessRecorder {
    fn record(&mut self, event: &AiAgentEvent) {
        match event {
            AiAgentEvent::ModelStarted { round } => {
                self.answer.clear();
                self.steps.push(PendingProcessStep {
                    id: model_step_id(*round),
                    kind: AiProcessStepKind::Model,
                    title: if *round == 1 {
                        "分析问题".into()
                    } else {
                        "分析日记内容".into()
                    },
                    detail: Some(if *round == 1 {
                        "理解问题并判断需要读取哪些日记".into()
                    } else {
                        "根据已读取的日记继续分析".into()
                    }),
                    reasoning: String::new(),
                    state: None,
                    duration_ms: None,
                });
            }
            AiAgentEvent::ReasoningDelta { round, delta } => {
                if let Some(step) = self.find_step_mut(&model_step_id(*round)) {
                    step.reasoning.push_str(delta);
                }
            }
            AiAgentEvent::ModelCompleted {
                round,
                tool_count,
                elapsed_ms,
            } => {
                if let Some(step) = self.find_step_mut(&model_step_id(*round)) {
                    if *tool_count == 0 {
                        step.title = "生成回答".into();
                    }
                    step.detail = Some(if *tool_count > 0 {
                        format!("决定执行 {tool_count} 个日记操作")
                    } else {
                        "回答生成完成".into()
                    });
                    step.state = Some(AiProcessStepState::Completed);
                    step.duration_ms = Some(*elapsed_ms);
                }
            }
            AiAgentEvent::ToolStarted {
                operation_id,
                title,
                detail,
                ..
            } => {
                self.answer.clear();
                self.steps.push(PendingProcessStep {
                    id: tool_step_id(*operation_id),
                    kind: AiProcessStepKind::Tool,
                    title: title.clone(),
                    detail: detail.clone(),
                    reasoning: String::new(),
                    state: None,
                    duration_ms: None,
                });
            }
            AiAgentEvent::ToolCompleted {
                operation_id,
                summary,
                succeeded,
                elapsed_ms,
            } => {
                if let Some(step) = self.find_step_mut(&tool_step_id(*operation_id)) {
                    step.detail = Some(summary.clone());
                    step.state = Some(if *succeeded {
                        AiProcessStepState::Completed
                    } else {
                        AiProcessStepState::Failed
                    });
                    step.duration_ms = Some(*elapsed_ms);
                }
            }
            AiAgentEvent::AnswerDelta(delta) => self.answer.push_str(delta),
            AiAgentEvent::ConversationSource(_)
            | AiAgentEvent::Completed(_)
            | AiAgentEvent::Failed(_)
            | AiAgentEvent::Cancelled => {}
        }
    }

    fn find_step_mut(&mut self, id: &str) -> Option<&mut PendingProcessStep> {
        self.steps.iter_mut().rev().find(|step| step.id == id)
    }

    fn finish(&self, unfinished_state: AiProcessStepState) -> Vec<AiProcessStepRecord> {
        self.steps
            .iter()
            .map(|step| AiProcessStepRecord {
                id: step.id.clone(),
                kind: step.kind,
                title: step.title.clone(),
                detail: step.detail.clone(),
                reasoning: step.reasoning.clone(),
                state: step.state.unwrap_or(unfinished_state),
                duration_ms: step.duration_ms,
            })
            .collect()
    }
}

fn finish_recorder(
    recorder: &Mutex<AiProcessRecorder>,
    unfinished_state: AiProcessStepState,
) -> Vec<AiProcessStepRecord> {
    recorder
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .finish(unfinished_state)
}

fn finish_recorder_with_answer(
    recorder: &Mutex<AiProcessRecorder>,
    unfinished_state: AiProcessStepState,
) -> (Vec<AiProcessStepRecord>, String) {
    let recorder = recorder
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (recorder.finish(unfinished_state), recorder.answer.clone())
}

fn model_step_id(round: usize) -> String {
    format!("model-{round}")
}

fn tool_step_id(operation_id: usize) -> String {
    format!("tool-{operation_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{
        AiAssistantMessage, AiCompletion, AiCompletionRequest, AiConversationSourceToolCall,
        AiModel, AiToolCall, AiToolDefinition, AiToolError,
    };
    use crate::app_object_store::{LocalAppObjectStore, SharedAppObjectStore};
    use crate::caches::LocalObjectStore;
    use crate::cryptos::Crypto;
    use async_trait::async_trait;
    use serde_json::Value;
    use std::collections::VecDeque;
    use std::sync::Arc;

    struct FakeProvider {
        responses: Mutex<VecDeque<Result<AiCompletion, AiError>>>,
        requests: Mutex<Vec<AiCompletionRequest>>,
    }

    impl FakeProvider {
        fn new(responses: Vec<Result<AiCompletion, AiError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                requests: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl AiModelProvider for FakeProvider {
        async fn list_models(&self) -> Result<Vec<AiModel>, AiError> {
            Ok(vec![])
        }

        async fn complete(&self, request: AiCompletionRequest) -> Result<AiCompletion, AiError> {
            self.requests.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("fake response")
        }
    }

    struct NoopTools;

    #[async_trait]
    impl AiToolExecutor for NoopTools {
        fn definitions(&self) -> Vec<AiToolDefinition> {
            vec![]
        }

        async fn execute(&self, _call: &AiToolCall) -> Result<Value, AiToolError> {
            unreachable!("no tools are exposed")
        }
    }

    struct PendingProvider;

    #[async_trait]
    impl AiModelProvider for PendingProvider {
        async fn list_models(&self) -> Result<Vec<AiModel>, AiError> {
            Ok(vec![])
        }

        async fn complete(&self, _request: AiCompletionRequest) -> Result<AiCompletion, AiError> {
            std::future::pending().await
        }
    }

    fn completion(answer: &str) -> AiCompletion {
        AiCompletion {
            message: AiAssistantMessage {
                reasoning_content: Some("思考".into()),
                content: Some(answer.into()),
                tool_calls: vec![],
            },
            finish_reason: Some("stop".into()),
            usage: None,
        }
    }

    fn repository() -> (tempfile::TempDir, AiSessionRepository) {
        let temp = tempfile::tempdir().unwrap();
        let crypto = Crypto::new();
        crypto
            .derive_dek(
                "session-agent-password".into(),
                "c2Vzc2lvbi1hZ2VudC10ZXN0LXNhbHQ",
            )
            .unwrap();
        let store: SharedAppObjectStore = Arc::new(LocalAppObjectStore::new(
            LocalObjectStore::new(temp.path().to_path_buf()),
        ));
        (temp, AiSessionRepository::new(store, crypto))
    }

    fn emit_to(
        events: &Mutex<Vec<AiAgentEvent>>,
    ) -> impl Fn(AiAgentEvent) -> Result<(), AiError> + '_ {
        move |event| {
            events.lock().unwrap().push(event);
            Ok(())
        }
    }

    #[tokio::test]
    async fn persists_completed_turns_and_reuses_completed_history() {
        let (_temp, repository) = repository();
        let session = repository
            .create_session("第一问".into(), "test-model".into(), 1)
            .await
            .unwrap();
        let provider = FakeProvider::new(vec![Ok(completion("第一答")), Ok(completion("第二答"))]);
        let tools = NoopTools;
        let events = Mutex::new(vec![]);

        for prompt in ["第一问", "第二问"] {
            let _run_guard = repository.try_begin_run(&session.id).unwrap();
            let outcome = AiSessionAgentRunner::new(&repository, &provider, &tools)
                .run(
                    &session.id,
                    prompt,
                    CancellationToken::new(),
                    &emit_to(&events),
                )
                .await
                .unwrap();
            assert!(matches!(outcome, AiSessionAgentOutcome::Completed { .. }));
        }

        let (_, messages) = repository.load_session(&session.id).await.unwrap().unwrap();
        assert_eq!(messages.len(), 4);
        let AiSessionMessagePayload::Assistant {
            state,
            process_steps,
            trace,
            ..
        } = &messages[1].payload
        else {
            panic!("assistant message")
        };
        assert_eq!(*state, AiAssistantRecordState::Completed);
        assert_eq!(process_steps.len(), 1);
        assert_eq!(process_steps[0].reasoning, "思考");
        assert_eq!(trace.len(), 1);

        let requests = provider.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].messages().len(), 3);
        assert_eq!(requests[1].messages().len(), 5);
        assert!(matches!(
            &requests[1].messages()[3],
            AiMessage::Assistant(AiAssistantMessage {
                reasoning_content: Some(reasoning),
                content: Some(answer),
                tool_calls,
            }) if reasoning == "思考" && answer == "第一答" && tool_calls.is_empty()
        ));
    }

    #[tokio::test]
    async fn persists_model_failures_and_cancellation_as_terminal_assistant_messages() {
        let (_temp, repository) = repository();
        let session = repository
            .create_session("失败测试".into(), "test-model".into(), 1)
            .await
            .unwrap();
        let provider = FakeProvider::new(vec![Err(AiError::RequestFailed("模拟失败".into()))]);
        let tools = NoopTools;
        let events = Mutex::new(vec![]);

        let failed = AiSessionAgentRunner::new(&repository, &provider, &tools)
            .run(
                &session.id,
                "失败问题",
                CancellationToken::new(),
                &emit_to(&events),
            )
            .await
            .unwrap();
        assert!(
            matches!(failed, AiSessionAgentOutcome::Failed(message) if message.contains("模拟失败"))
        );

        let cancelled_token = CancellationToken::new();
        cancelled_token.cancel();
        let cancelled = AiSessionAgentRunner::new(&repository, &PendingProvider, &tools)
            .run(&session.id, "取消问题", cancelled_token, &emit_to(&events))
            .await
            .unwrap();
        assert!(matches!(cancelled, AiSessionAgentOutcome::Cancelled));

        let (_, messages) = repository.load_session(&session.id).await.unwrap().unwrap();
        assert_eq!(messages.len(), 4);
        assert!(matches!(
            messages[1].payload,
            AiSessionMessagePayload::Assistant {
                state: AiAssistantRecordState::Failed,
                ..
            }
        ));
        assert!(matches!(
            messages[3].payload,
            AiSessionMessagePayload::Assistant {
                state: AiAssistantRecordState::Cancelled,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn repairs_a_trailing_user_message_before_starting_the_next_turn() {
        let (_temp, repository) = repository();
        let session = repository
            .create_session("恢复测试".into(), "test-model".into(), 1)
            .await
            .unwrap();
        repository
            .append_message(
                &session.id,
                2,
                AiSessionMessagePayload::User {
                    content: "中断的问题".into(),
                    timezone_offset_minutes: None,
                },
            )
            .await
            .unwrap();
        let provider = FakeProvider::new(vec![Ok(completion("新回答"))]);
        let events = Mutex::new(vec![]);

        AiSessionAgentRunner::new(&repository, &provider, &NoopTools)
            .run(
                &session.id,
                "新问题",
                CancellationToken::new(),
                &emit_to(&events),
            )
            .await
            .unwrap();

        let (_, messages) = repository.load_session(&session.id).await.unwrap().unwrap();
        assert_eq!(messages.len(), 4);
        assert!(matches!(
            &messages[1].payload,
            AiSessionMessagePayload::Assistant {
                state: AiAssistantRecordState::Failed,
                error: Some(error),
                ..
            } if error == INTERRUPTED_ANSWER_ERROR
        ));
        assert_eq!(provider.requests.lock().unwrap()[0].messages().len(), 3);
    }

    #[tokio::test]
    async fn replays_historical_tool_calls_and_results_without_executing_them_again() {
        let (_temp, repository) = repository();
        let session = repository
            .create_session("第一问".into(), "test-model".into(), 1)
            .await
            .unwrap();
        let now = now_millis();
        repository
            .append_message(
                &session.id,
                now - 2,
                AiSessionMessagePayload::User {
                    content: "第一问".into(),
                    timezone_offset_minutes: current_timezone_offset_minutes(),
                },
            )
            .await
            .unwrap();
        repository
            .append_message(
                &session.id,
                now - 1,
                AiSessionMessagePayload::Assistant {
                    state: AiAssistantRecordState::Completed,
                    content: "第一答".into(),
                    error: None,
                    model: "old-model".into(),
                    usage: None,
                    process_steps: vec![],
                    trace: vec![
                        AiConversationSourceMessage::Assistant {
                            reasoning_content: Some("先读取日记".into()),
                            content: None,
                            tool_calls: vec![AiConversationSourceToolCall {
                                id: "call-1".into(),
                                name: "read_diary".into(),
                                arguments: r#"{"diaryId":"123"}"#.into(),
                            }],
                        },
                        AiConversationSourceMessage::Tool {
                            tool_call_id: "call-1".into(),
                            content: r#"{"ok":true,"result":{"content":"正文"}}"#.into(),
                        },
                        AiConversationSourceMessage::Assistant {
                            reasoning_content: None,
                            content: Some("第一答".into()),
                            tool_calls: vec![],
                        },
                    ],
                },
            )
            .await
            .unwrap();
        let provider = FakeProvider::new(vec![Ok(completion("第二答"))]);
        let events = Mutex::new(vec![]);

        AiSessionAgentRunner::new(&repository, &provider, &NoopTools)
            .run(
                &session.id,
                "第二问",
                CancellationToken::new(),
                &emit_to(&events),
            )
            .await
            .unwrap();

        let requests = provider.requests.lock().unwrap();
        let messages = requests[0].messages();
        assert_eq!(messages.len(), 7);
        assert!(matches!(messages[1], AiMessage::System(_)));
        assert_eq!(messages[2], AiMessage::User("第一问".into()));
        assert!(matches!(
            &messages[3],
            AiMessage::Assistant(message)
                if message.tool_calls.len() == 1
                    && message.tool_calls[0].id == "call-1"
                    && message.reasoning_content.as_deref() == Some("先读取日记")
        ));
        assert!(matches!(
            &messages[4],
            AiMessage::Tool(result)
                if result.tool_call_id == "call-1" && result.content.contains("正文")
        ));
        assert_eq!(messages[5], final_answer_message("第一答"));
        assert_eq!(messages[6], AiMessage::User("第二问".into()));
    }

    #[test]
    fn invalid_historical_trace_falls_back_to_the_final_answer() {
        let messages = vec![
            user_message(0, 1_700_000_000_000, "问题", Some(480)),
            assistant_message(
                1,
                1_700_000_001_000,
                "最终回答",
                vec![AiConversationSourceMessage::Assistant {
                    reasoning_content: None,
                    content: None,
                    tool_calls: vec![AiConversationSourceToolCall {
                        id: "call-without-result".into(),
                        name: "read_diary".into(),
                        arguments: "{}".into(),
                    }],
                }],
            ),
        ];

        let history = conversation_history(&messages, false).unwrap();

        assert_eq!(history.completed_turns, 1);
        assert_eq!(history.messages.len(), 3);
        assert_eq!(history.messages[1], AiMessage::User("问题".into()));
        assert_eq!(history.messages[2], final_answer_message("最终回答"));
    }

    #[test]
    fn inserts_deterministic_time_context_only_for_the_first_and_changed_dates() {
        let offset = FixedOffset::east_opt(8 * 60 * 60).unwrap();
        let timestamp = |year, month, day, hour, minute| {
            offset
                .with_ymd_and_hms(year, month, day, hour, minute, 0)
                .single()
                .unwrap()
                .timestamp_millis()
        };
        let messages = vec![
            user_message(0, timestamp(2026, 8, 22, 23, 0), "第一问", Some(480)),
            assistant_message(1, timestamp(2026, 8, 22, 23, 1), "第一答", vec![]),
            user_message(2, timestamp(2026, 8, 23, 0, 2), "第二问", Some(480)),
            assistant_message(3, timestamp(2026, 8, 23, 0, 3), "第二答", vec![]),
            user_message(4, timestamp(2026, 8, 23, 10, 0), "当前问题", Some(480)),
        ];

        let history = conversation_history(&messages, true).unwrap();
        let time_contexts = history
            .messages
            .iter()
            .filter_map(|message| match message {
                AiMessage::System(content) => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(time_contexts.len(), 2);
        assert!(time_contexts[0].contains("2026-08-22 23:00"));
        assert!(time_contexts[1].contains("2026-08-23 00:02"));
        assert!(time_contexts
            .iter()
            .all(|message| !message.contains("跨过")));
        assert!(!history
            .messages
            .iter()
            .any(|message| message == &AiMessage::User("当前问题".into())));
    }

    fn user_message(
        index: u64,
        created_at: i64,
        content: &str,
        timezone_offset_minutes: Option<i16>,
    ) -> AiSessionMessage {
        AiSessionMessage {
            index,
            created_at,
            payload: AiSessionMessagePayload::User {
                content: content.into(),
                timezone_offset_minutes,
            },
        }
    }

    fn assistant_message(
        index: u64,
        created_at: i64,
        content: &str,
        trace: Vec<AiConversationSourceMessage>,
    ) -> AiSessionMessage {
        AiSessionMessage {
            index,
            created_at,
            payload: AiSessionMessagePayload::Assistant {
                state: AiAssistantRecordState::Completed,
                content: content.into(),
                error: None,
                model: "test-model".into(),
                usage: None,
                process_steps: vec![],
                trace,
            },
        }
    }

    #[test]
    fn recorder_preserves_tool_failure_and_finishes_only_running_steps() {
        let mut recorder = AiProcessRecorder::default();
        recorder.record(&AiAgentEvent::ModelStarted { round: 1 });
        recorder.record(&AiAgentEvent::ReasoningDelta {
            round: 1,
            delta: "先搜索".into(),
        });
        recorder.record(&AiAgentEvent::ModelCompleted {
            round: 1,
            tool_count: 1,
            elapsed_ms: 10,
        });
        recorder.record(&AiAgentEvent::ToolStarted {
            operation_id: 1,
            round: 1,
            title: "搜索日记".into(),
            detail: None,
        });
        recorder.record(&AiAgentEvent::ToolCompleted {
            operation_id: 1,
            summary: "搜索失败".into(),
            succeeded: false,
            elapsed_ms: 5,
        });
        recorder.record(&AiAgentEvent::ModelStarted { round: 2 });

        let steps = recorder.finish(AiProcessStepState::Cancelled);
        assert_eq!(steps[0].state, AiProcessStepState::Completed);
        assert_eq!(steps[1].state, AiProcessStepState::Failed);
        assert_eq!(steps[2].state, AiProcessStepState::Cancelled);
        assert_eq!(steps[0].reasoning, "先搜索");
    }
}
