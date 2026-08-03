import type {Channel} from '@tauri-apps/api/core';
import type {AiAgentEvent, AiAgentResponse} from '../bindings';
import api from './api';
import type {AiServiceConfig} from './aiConfig';

export type AiExchangeState = 'running' | 'canceling' | 'completed' | 'failed' | 'cancelled';
export type AiProcessStepState = 'running' | 'completed' | 'failed' | 'cancelled';
export type AiProcessStepKind = 'model' | 'tool';

export interface AiProcessStep {
  id: string;
  kind: AiProcessStepKind;
  title: string;
  detail: string | null;
  reasoning: string;
  state: AiProcessStepState;
  durationMs: number | null;
}

export interface AiAgentDisplayState {
  state: AiExchangeState;
  answer: string;
  status: string;
  response: AiAgentResponse | null;
  error: string | null;
  processSteps: AiProcessStep[];
}

export type AiAgentRunner = (
  event: Channel<AiAgentEvent>,
  baseUrl: string,
  apiKey: string | null,
  model: string,
  prompt: string,
) => Promise<string>;

export async function startAiQuestion(
  config: AiServiceConfig,
  prompt: string,
  event: Channel<AiAgentEvent>,
  runner: AiAgentRunner = api.cmdRunAiAgent,
): Promise<string> {
  const normalizedPrompt = prompt.trim();
  if (!normalizedPrompt) {
    throw new Error('问题不能为空');
  }

  return runner(
    event,
    config.baseUrl,
    config.apiKey.trim() || null,
    config.model,
    normalizedPrompt,
  );
}

export function initialAiAgentDisplayState(): AiAgentDisplayState {
  return {
    state: 'running',
    answer: '',
    status: '正在连接 AI 服务…',
    response: null,
    error: null,
    processSteps: [],
  };
}

export function reduceAiAgentEvent(
  state: AiAgentDisplayState,
  message: AiAgentEvent,
): AiAgentDisplayState {
  if (isTerminalAiExchangeState(state.state)) return state;

  switch (message.event) {
    case 'modelStarted':
      if (state.state === 'canceling') return state;
      return {
        ...state,
        answer: '',
        processSteps: [...state.processSteps, {
          id: modelStepId(message.data.round),
          kind: 'model',
          title: message.data.round === 1 ? '分析问题' : '分析日记内容',
          detail: message.data.round === 1
            ? '理解问题并判断需要读取哪些日记'
            : '根据已读取的日记继续分析',
          reasoning: '',
          state: 'running',
          durationMs: null,
        }],
        status: message.data.round === 1
          ? 'AI 正在理解问题…'
          : 'AI 正在分析日记内容…',
      };
    case 'reasoningDelta':
      if (state.state === 'canceling') return state;
      return {
        ...state,
        processSteps: updateProcessStep(
          state.processSteps,
          modelStepId(message.data.round),
          step => ({
            ...step,
            reasoning: step.reasoning + message.data.delta,
          }),
        ),
        status: 'AI 正在思考…',
      };
    case 'modelCompleted':
      if (state.state === 'canceling') return state;
      return {
        ...state,
        answer: message.data.toolCount > 0 ? '' : state.answer,
        processSteps: updateProcessStep(
          state.processSteps,
          modelStepId(message.data.round),
          step => ({
            ...step,
            title: message.data.toolCount === 0 ? '生成回答' : step.title,
            detail: formatModelResultDetail(message.data.toolCount),
            state: 'completed',
            durationMs: message.data.elapsedMs,
          }),
        ),
        status: message.data.toolCount > 0
          ? '正在准备读取日记…'
          : '正在完成回答…',
      };
    case 'toolStarted':
      if (state.state === 'canceling') return state;
      return {
        ...state,
        answer: '',
        processSteps: [...state.processSteps, {
          id: toolStepId(message.data.operationId),
          kind: 'tool',
          title: message.data.title,
          detail: message.data.detail,
          reasoning: '',
          state: 'running',
          durationMs: null,
        }],
        status: `正在${message.data.title}…`,
      };
    case 'toolCompleted':
      if (state.state === 'canceling') return state;
      return {
        ...state,
        processSteps: updateProcessStep(
          state.processSteps,
          toolStepId(message.data.operationId),
          step => ({
            ...step,
            detail: message.data.summary,
            state: message.data.succeeded ? 'completed' : 'failed',
            durationMs: message.data.elapsedMs,
          }),
        ),
        status: message.data.succeeded
          ? '正在继续分析…'
          : '日记操作失败，AI 正在继续处理…',
      };
    case 'answerDelta':
      if (state.state === 'canceling') return state;
      return {
        ...state,
        answer: state.answer + message.data,
        status: '正在生成回答…',
      };
    case 'completed':
      return {
        state: 'completed',
        answer: message.data.answer,
        status: '',
        response: message.data,
        error: null,
        processSteps: state.processSteps,
      };
    case 'failed':
      return {
        ...state,
        state: 'failed',
        status: '',
        error: message.data,
        processSteps: finishRunningProcessSteps(state.processSteps, 'failed'),
      };
    case 'cancelled':
      return {
        ...state,
        state: 'cancelled',
        status: '',
        error: null,
        processSteps: finishRunningProcessSteps(state.processSteps, 'cancelled'),
      };
  }
}

export function isTerminalAiExchangeState(
  state: AiExchangeState,
): state is Extract<AiExchangeState, 'completed' | 'failed' | 'cancelled'> {
  return state === 'completed' || state === 'failed' || state === 'cancelled';
}

export function shouldCollapseAiProcess(
  state: AiAgentDisplayState,
  message: AiAgentEvent,
): boolean {
  return message.event === 'answerDelta' && state.answer.length === 0;
}

export function formatAiResponseMeta(response: AiAgentResponse): string {
  if (!response.usage) return '';
  return `输入 ${response.usage.promptTokens} Token · 输出 ${response.usage.completionTokens} Token`;
}

export function formatAiProcessSummary(steps: AiProcessStep[]): string {
  const toolCount = steps.filter(step => step.kind === 'tool').length;
  const durationMs = steps.reduce((total, step) => total + (step.durationMs ?? 0), 0);
  const parts: string[] = [];
  if (toolCount > 0) parts.push(`${toolCount} 次日记操作`);
  if (durationMs > 0) parts.push(formatProcessDuration(durationMs));
  return parts.length > 0 ? parts.join(' · ') : '处理完成';
}

export function formatProcessDuration(durationMs: number): string {
  if (durationMs < 1000) return `${Math.max(0, Math.round(durationMs))} 毫秒`;
  if (durationMs < 60_000) {
    const seconds = (durationMs / 1000).toFixed(1).replace(/\.0$/, '');
    return `${seconds} 秒`;
  }
  const minutes = Math.floor(durationMs / 60_000);
  const seconds = Math.round((durationMs % 60_000) / 1000);
  return seconds > 0 ? `${minutes} 分 ${seconds} 秒` : `${minutes} 分钟`;
}

function modelStepId(round: number): string {
  return `model-${round}`;
}

function toolStepId(operationId: number): string {
  return `tool-${operationId}`;
}

function formatModelResultDetail(toolCount: number): string {
  return toolCount > 0
    ? `决定执行 ${toolCount} 个日记操作`
    : '回答生成完成';
}

function updateProcessStep(
  steps: AiProcessStep[],
  id: string,
  update: (step: AiProcessStep) => AiProcessStep,
): AiProcessStep[] {
  return steps.map(step => step.id === id ? update(step) : step);
}

function finishRunningProcessSteps(
  steps: AiProcessStep[],
  state: Extract<AiProcessStepState, 'failed' | 'cancelled'>,
): AiProcessStep[] {
  return steps.map(step => step.state === 'running' ? {...step, state} : step);
}
