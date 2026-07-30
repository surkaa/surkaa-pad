import {describe, expect, it, vi} from 'vitest';
import type {Channel} from '@tauri-apps/api/core';
import type {AiAgentEvent} from '../../bindings';
import {
  formatAiResponseMeta,
  initialAiAgentDisplayState,
  reduceAiAgentEvent,
  startAiQuestion,
} from '../aiAssistant';
import type {AiServiceConfig} from '../aiConfig';

const config: AiServiceConfig = {
  baseUrl: 'http://localhost:11434/v1',
  apiKey: ' local-secret ',
  model: 'qwen3:8b',
};

describe('startAiQuestion', () => {
  it('trims the question and forwards the configured service', async () => {
    const event = {} as Channel<AiAgentEvent>;
    const runner = vi.fn().mockResolvedValue('task-token');

    await expect(startAiQuestion(config, '  最近写了什么？\n', event, runner))
      .resolves.toBe('task-token');
    expect(runner).toHaveBeenCalledWith(
      event,
      'http://localhost:11434/v1',
      'local-secret',
      'qwen3:8b',
      '最近写了什么？',
    );
  });

  it('omits a blank API key and rejects blank questions', async () => {
    const event = {} as Channel<AiAgentEvent>;
    const runner = vi.fn().mockResolvedValue('task-token');

    await startAiQuestion({...config, apiKey: '  '}, '问题', event, runner);
    expect(runner).toHaveBeenCalledWith(
      event,
      config.baseUrl,
      null,
      config.model,
      '问题',
    );
    await expect(startAiQuestion(config, ' \n ', event, runner)).rejects.toThrow('问题不能为空');
  });
});

describe('reduceAiAgentEvent', () => {
  it('streams text, clears temporary tool preambles, and completes with final metadata', () => {
    let state = initialAiAgentDisplayState();
    state = reduceAiAgentEvent(state, {
      event: 'modelStarted',
      data: {round: 1},
    });
    state = reduceAiAgentEvent(state, {event: 'answerDelta', data: '我先查找'});
    expect(state.answer).toBe('我先查找');

    state = reduceAiAgentEvent(state, {
      event: 'toolExecutionStarted',
      data: {round: 1, toolCount: 2},
    });
    expect(state.answer).toBe('');
    expect(state.status).toContain('2 个日记读取操作');

    state = reduceAiAgentEvent(state, {
      event: 'modelStarted',
      data: {round: 2},
    });
    state = reduceAiAgentEvent(state, {event: 'answerDelta', data: '最终'});
    state = reduceAiAgentEvent(state, {event: 'answerDelta', data: '回答'});
    expect(state.answer).toBe('最终回答');

    const response = {answer: '最终回答', modelRounds: 2, usage: null};
    state = reduceAiAgentEvent(state, {event: 'completed', data: response});
    expect(state).toEqual({
      state: 'completed',
      answer: '最终回答',
      status: '',
      response,
      error: null,
    });
  });

  it('ignores late deltas while canceling but still accepts terminal events', () => {
    const canceling = {
      ...initialAiAgentDisplayState(),
      state: 'canceling' as const,
      answer: '已生成部分',
      status: '正在停止生成…',
    };

    expect(reduceAiAgentEvent(canceling, {
      event: 'answerDelta',
      data: '不应追加',
    })).toEqual(canceling);
    expect(reduceAiAgentEvent(canceling, {event: 'cancelled'})).toMatchObject({
      state: 'cancelled',
      answer: '已生成部分',
      status: '',
    });
  });

  it('keeps a backend failure message for display', () => {
    const failed = reduceAiAgentEvent(initialAiAgentDisplayState(), {
      event: 'failed',
      data: '模型服务断开连接',
    });

    expect(failed.state).toBe('failed');
    expect(failed.error).toBe('模型服务断开连接');
  });
});

describe('formatAiResponseMeta', () => {
  it('shows model rounds and optional token usage', () => {
    expect(formatAiResponseMeta({
      answer: '回答',
      modelRounds: 3,
      usage: {promptTokens: 20, completionTokens: 8, totalTokens: 28},
    })).toBe('3 次模型调用 · 28 Token');
    expect(formatAiResponseMeta({answer: '回答', modelRounds: 1, usage: null}))
      .toBe('1 次模型调用');
  });
});
