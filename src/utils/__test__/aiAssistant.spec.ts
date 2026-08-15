import {describe, expect, it, vi} from 'vitest';
import type {Channel} from '@tauri-apps/api/core';
import type {AiAgentEvent} from '../../bindings';
import {
  buildAiConversationHistory,
  formatAiResponseMeta,
  formatAiConversationSource,
  formatAiProcessSummary,
  formatProcessDuration,
  initialAiAgentDisplayState,
  nextAiProcessExpanded,
  reduceAiAgentEvent,
  shouldCollapseAiProcess,
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

    const history = [{user: '上一问', assistant: '上一答'}];
    await expect(startAiQuestion(config, '  最近写了什么？\n', history, event, runner))
      .resolves.toBe('task-token');
    expect(runner).toHaveBeenCalledWith(
      event,
      'http://localhost:11434/v1',
      'local-secret',
      'qwen3:8b',
      history,
      '最近写了什么？',
    );
  });

  it('omits a blank API key and rejects blank questions', async () => {
    const event = {} as Channel<AiAgentEvent>;
    const runner = vi.fn().mockResolvedValue('task-token');

    await startAiQuestion({...config, apiKey: '  '}, '问题', [], event, runner);
    expect(runner).toHaveBeenCalledWith(
      event,
      config.baseUrl,
      null,
      config.model,
      [],
      '问题',
    );
    await expect(startAiQuestion(config, ' \n ', [], event, runner)).rejects.toThrow('问题不能为空');
  });
});

describe('buildAiConversationHistory', () => {
  it('keeps only completed non-empty question and answer pairs in order', () => {
    expect(buildAiConversationHistory([
      {state: 'completed', question: ' 第一问 ', answer: ' 第一答 '},
      {state: 'failed', question: '失败问题', answer: '部分回答'},
      {state: 'cancelled', question: '取消问题', answer: '部分回答'},
      {state: 'running', question: '当前问题', answer: ''},
      {state: 'completed', question: ' ', answer: '空问题'},
      {state: 'completed', question: '第二问', answer: '第二答'},
    ])).toEqual([
      {user: '第一问', assistant: '第一答'},
      {user: '第二问', assistant: '第二答'},
    ]);
  });
});

describe('formatAiConversationSource', () => {
  it('formats the complete message chain as readable JSON', () => {
    const source = {
      model: 'qwen3:8b',
      messages: [
        {role: 'system' as const, content: '系统提示'},
        {role: 'user' as const, content: '读取日记'},
        {
          role: 'tool' as const,
          tool_call_id: 'call-1',
          content: '{"ok":true}',
        },
      ],
    };

    expect(formatAiConversationSource(source)).toBe(JSON.stringify(source, null, 2));
  });
});

describe('reduceAiAgentEvent', () => {
  it('streams text, clears temporary tool preambles, and completes with final metadata', () => {
    let state = initialAiAgentDisplayState();
    state = reduceAiAgentEvent(state, {
      event: 'modelStarted',
      data: {round: 1},
    });
    state = reduceAiAgentEvent(state, {
      event: 'reasoningDelta',
      data: {round: 1, delta: '先理解问题，'},
    });
    state = reduceAiAgentEvent(state, {
      event: 'reasoningDelta',
      data: {round: 1, delta: '再决定搜索。'},
    });
    expect(state.processSteps[0].reasoning).toBe('先理解问题，再决定搜索。');
    expect(state.status).toBe('AI 正在思考…');
    state = reduceAiAgentEvent(state, {event: 'answerDelta', data: '我先查找'});
    expect(state.answer).toBe('我先查找');

    state = reduceAiAgentEvent(state, {
      event: 'modelCompleted',
      data: {
        round: 1,
        toolCount: 1,
        elapsedMs: 1200,
      },
    });
    expect(state.answer).toBe('');
    expect(state.processSteps[0]).toMatchObject({
      id: 'model-1',
      title: '分析问题',
      state: 'completed',
      durationMs: 1200,
      detail: '决定执行 1 个日记操作',
      reasoning: '先理解问题，再决定搜索。',
    });

    state = reduceAiAgentEvent(state, {
      event: 'toolStarted',
      data: {
        operationId: 1,
        round: 1,
        title: '搜索日记',
        detail: '“今天 下雨”',
      },
    });
    expect(state.status).toBe('正在搜索日记…');
    state = reduceAiAgentEvent(state, {
      event: 'toolCompleted',
      data: {
        operationId: 1,
        summary: '找到 2 篇日记',
        succeeded: true,
        elapsedMs: 320,
      },
    });
    expect(state.processSteps[1]).toMatchObject({
      id: 'tool-1',
      state: 'completed',
      detail: '找到 2 篇日记',
      durationMs: 320,
    });

    state = reduceAiAgentEvent(state, {
      event: 'modelStarted',
      data: {round: 2},
    });
    state = reduceAiAgentEvent(state, {event: 'answerDelta', data: '最终'});
    state = reduceAiAgentEvent(state, {event: 'answerDelta', data: '回答'});
    state = reduceAiAgentEvent(state, {
      event: 'modelCompleted',
      data: {round: 2, toolCount: 0, elapsedMs: 850},
    });
    expect(state.answer).toBe('最终回答');
    expect(state.processSteps[2].title).toBe('生成回答');

    const response = {answer: '最终回答', modelRounds: 2, usage: null};
    state = reduceAiAgentEvent(state, {event: 'completed', data: response});
    expect(state).toEqual({
      state: 'completed',
      answer: '最终回答',
      status: '',
      response,
      error: null,
      processSteps: state.processSteps,
    });
    expect(formatAiProcessSummary(state.processSteps))
      .toBe('1 次日记操作 · 2.4 秒');
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

  it('marks active process steps when a request fails or is cancelled', () => {
    const running = reduceAiAgentEvent(initialAiAgentDisplayState(), {
      event: 'modelStarted',
      data: {round: 1},
    });

    expect(reduceAiAgentEvent(running, {event: 'failed', data: '连接失败'})
      .processSteps[0].state).toBe('failed');
    expect(reduceAiAgentEvent(running, {event: 'cancelled'})
      .processSteps[0].state).toBe('cancelled');
  });

  it('keeps failed tool operations visible while the agent continues', () => {
    let state = reduceAiAgentEvent(initialAiAgentDisplayState(), {
      event: 'toolStarted',
      data: {operationId: 1, round: 1, title: '读取日记', detail: '日记 123'},
    });
    state = reduceAiAgentEvent(state, {
      event: 'toolCompleted',
      data: {
        operationId: 1,
        summary: '操作失败，AI 将根据现有信息继续处理',
        succeeded: false,
        elapsedMs: 40,
      },
    });

    expect(state.state).toBe('running');
    expect(state.processSteps[0]).toMatchObject({state: 'failed', durationMs: 40});
    expect(state.status).toContain('继续处理');
  });

  it('keeps a backend failure message for display', () => {
    const failed = reduceAiAgentEvent(initialAiAgentDisplayState(), {
      event: 'failed',
      data: '模型服务断开连接',
    });

    expect(failed.state).toBe('failed');
    expect(failed.error).toBe('模型服务断开连接');
  });

  it('leaves display state unchanged when the full source arrives', () => {
    const state = initialAiAgentDisplayState();
    const source = {
      model: 'qwen3:8b',
      messages: [{role: 'system' as const, content: '系统提示'}],
    };

    expect(reduceAiAgentEvent(state, {
      event: 'conversationSource',
      data: source,
    })).toBe(state);
  });
});

describe('process formatting', () => {
  it('formats compact durations and process totals', () => {
    expect(formatProcessDuration(999)).toBe('999 毫秒');
    expect(formatProcessDuration(1500)).toBe('1.5 秒');
    expect(formatProcessDuration(60_000)).toBe('1 分钟');
    expect(formatProcessDuration(65_000)).toBe('1 分 5 秒');
    expect(formatAiProcessSummary([])).toBe('处理完成');
  });
});

describe('process visibility', () => {
  it('collapses only when the first answer text arrives', () => {
    const state = initialAiAgentDisplayState();

    expect(shouldCollapseAiProcess(state, {
      event: 'reasoningDelta',
      data: {round: 1, delta: '思考中'},
    })).toBe(false);
    expect(shouldCollapseAiProcess(state, {
      event: 'answerDelta',
      data: '第一段正文',
    })).toBe(true);
    expect(shouldCollapseAiProcess({...state, answer: '已有正文'}, {
      event: 'answerDelta',
      data: '后续正文',
    })).toBe(false);
  });

  it('does not reopen automatically in later model rounds', () => {
    let state = initialAiAgentDisplayState();
    let expanded = true;
    const answerEvent = {event: 'answerDelta', data: '正文'} as const;

    expanded = nextAiProcessExpanded(expanded, state, answerEvent);
    state = reduceAiAgentEvent(state, answerEvent);
    expect(expanded).toBe(false);

    expanded = nextAiProcessExpanded(expanded, state, {
      event: 'modelStarted',
      data: {round: 2},
    });
    expect(expanded).toBe(false);
  });
});

describe('formatAiResponseMeta', () => {
  it('shows input, output, and total token usage when available', () => {
    expect(formatAiResponseMeta({
      answer: '回答',
      modelRounds: 3,
      usage: {promptTokens: 20, completionTokens: 8, totalTokens: 28},
    })).toBe('输入 20 Token · 输出 8 Token');
    expect(formatAiResponseMeta({answer: '回答', modelRounds: 1, usage: null}))
      .toBe('');
  });
});
