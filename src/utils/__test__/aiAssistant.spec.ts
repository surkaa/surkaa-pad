import {describe, expect, it, vi} from 'vitest';
import type {AiAgentResponse} from '../../bindings';
import {formatAiResponseMeta, runAiQuestion} from '../aiAssistant';
import type {AiServiceConfig} from '../aiConfig';

const config: AiServiceConfig = {
  baseUrl: 'http://localhost:11434/v1',
  apiKey: ' local-secret ',
  model: 'qwen3:8b',
};

describe('runAiQuestion', () => {
  it('trims the question and forwards the configured service', async () => {
    const response: AiAgentResponse = {
      answer: '回答',
      modelRounds: 2,
      usage: null,
    };
    const runner = vi.fn().mockResolvedValue(response);

    await expect(runAiQuestion(config, '  最近写了什么？\n', runner))
      .resolves.toEqual(response);
    expect(runner).toHaveBeenCalledWith(
      'http://localhost:11434/v1',
      'local-secret',
      'qwen3:8b',
      '最近写了什么？',
    );
  });

  it('omits a blank API key and rejects blank questions', async () => {
    const runner = vi.fn().mockResolvedValue({
      answer: '回答',
      modelRounds: 1,
      usage: null,
    });

    await runAiQuestion({...config, apiKey: '  '}, '问题', runner);
    expect(runner).toHaveBeenCalledWith(
      config.baseUrl,
      null,
      config.model,
      '问题',
    );
    await expect(runAiQuestion(config, ' \n ', runner)).rejects.toThrow('问题不能为空');
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
