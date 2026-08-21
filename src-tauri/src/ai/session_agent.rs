use super::{
    AiAgent, AiAgentEvent, AiAgentResponse, AiAssistantRecordState, AiConversationSource,
    AiConversationSourceMessage, AiConversationTurn, AiError, AiModelProvider, AiProcessStepKind,
    AiProcessStepRecord, AiProcessStepState, AiSessionMessage, AiSessionMessagePayload,
    AiSessionRepository, AiSessionRepositoryError, AiToolExecutor,
};
use chrono::Utc;
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

        let history_started_at = Instant::now();
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
        let history = conversation_history(&stored_messages)?;
        let history_prepare_ms = history_started_at.elapsed().as_millis();
        let history_turns = history.len();
        let model = meta.model;

        let user_save_started_at = Instant::now();
        self.repository
            .append_message(
                session_id,
                now_millis(),
                AiSessionMessagePayload::User {
                    content: prompt.to_owned(),
                },
            )
            .await?;
        let user_save_ms = user_save_started_at.elapsed().as_millis();

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
            let run =
                agent.run_stream_with_history_source(&model, &history, prompt, &recorded_emit);
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

fn conversation_history(
    messages: &[AiSessionMessage],
) -> Result<Vec<AiConversationTurn>, AiSessionAgentError> {
    validate_message_order(messages, false)?;
    let mut history = Vec::with_capacity(messages.len() / 2);
    for pair in messages.chunks_exact(2) {
        let AiSessionMessagePayload::User { content: user } = &pair[0].payload else {
            unreachable!("message order was validated")
        };
        let AiSessionMessagePayload::Assistant { state, content, .. } = &pair[1].payload else {
            unreachable!("message order was validated")
        };
        if *state == AiAssistantRecordState::Completed {
            history.push(AiConversationTurn {
                user: user.clone(),
                assistant: content.clone(),
            });
        }
    }
    Ok(history)
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
        AiAssistantMessage, AiCompletion, AiCompletionRequest, AiModel, AiToolCall,
        AiToolDefinition, AiToolError,
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
    async fn persists_completed_turns_and_reuses_only_completed_history() {
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
        assert_eq!(requests[0].messages().len(), 2);
        assert_eq!(requests[1].messages().len(), 4);
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
        assert_eq!(provider.requests.lock().unwrap()[0].messages().len(), 2);
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
