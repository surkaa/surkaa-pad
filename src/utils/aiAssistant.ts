import type {Channel} from '@tauri-apps/api/core';
import type {AiAgentEvent, AiAgentResponse} from '../bindings';
import api from './api';
import type {AiServiceConfig} from './aiConfig';

export type AiExchangeState = 'running' | 'canceling' | 'completed' | 'failed' | 'cancelled';

export interface AiAgentDisplayState {
  state: AiExchangeState;
  answer: string;
  status: string;
  response: AiAgentResponse | null;
  error: string | null;
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
        status: message.data.round === 1
          ? '正在调用模型…'
          : '正在根据日记内容生成回答…',
      };
    case 'toolExecutionStarted':
      if (state.state === 'canceling') return state;
      return {
        ...state,
        answer: '',
        status: message.data.toolCount > 1
          ? `正在执行 ${message.data.toolCount} 个日记读取操作…`
          : '正在读取相关日记…',
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
      };
    case 'failed':
      return {...state, state: 'failed', status: '', error: message.data};
    case 'cancelled':
      return {...state, state: 'cancelled', status: '', error: null};
  }
}

export function isTerminalAiExchangeState(
  state: AiExchangeState,
): state is Extract<AiExchangeState, 'completed' | 'failed' | 'cancelled'> {
  return state === 'completed' || state === 'failed' || state === 'cancelled';
}

export function formatAiResponseMeta(response: AiAgentResponse): string {
  const parts = [`${response.modelRounds} 次模型调用`];
  if (response.usage) {
    parts.push(`${response.usage.totalTokens} Token`);
  }
  return parts.join(' · ');
}
