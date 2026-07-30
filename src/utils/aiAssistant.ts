import type {AiAgentResponse} from '../bindings';
import api from './api';
import type {AiServiceConfig} from './aiConfig';

export type AiAgentRunner = (
  baseUrl: string,
  apiKey: string | null,
  model: string,
  prompt: string,
) => Promise<AiAgentResponse>;

export async function runAiQuestion(
  config: AiServiceConfig,
  prompt: string,
  runner: AiAgentRunner = api.cmdRunAiAgent,
): Promise<AiAgentResponse> {
  const normalizedPrompt = prompt.trim();
  if (!normalizedPrompt) {
    throw new Error('问题不能为空');
  }

  return runner(
    config.baseUrl,
    config.apiKey.trim() || null,
    config.model,
    normalizedPrompt,
  );
}

export function formatAiResponseMeta(response: AiAgentResponse): string {
  const parts = [`${response.modelRounds} 次模型调用`];
  if (response.usage) {
    parts.push(`${response.usage.totalTokens} Token`);
  }
  return parts.join(' · ');
}
