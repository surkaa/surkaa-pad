import {useConfigStore} from '../stores/config';
import api from './api';

export const DEFAULT_AI_BASE_URL = 'http://localhost:11434/v1';

export interface AiServiceConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
}

export type AiEndpointSecurity = 'secure' | 'localHttp' | 'remoteHttp';

export interface AiConfigStorage {
  read(): Promise<number[] | null>;
  write(value: number[]): Promise<void>;
  remove(): Promise<void>;
}

export interface AiConfigCipher {
  encrypt(plaintext: string): Promise<number[]>;
  decrypt(encrypted: number[]): Promise<string>;
}

export type AiModelLister = typeof api.cmdListAiModels;

export function normalizeAiServiceConfig(value: unknown): AiServiceConfig {
  if (!value || typeof value !== 'object') {
    throw new Error('AI 服务配置格式无效');
  }
  const input = value as Partial<AiServiceConfig>;
  const baseUrl = typeof input.baseUrl === 'string' ? input.baseUrl.trim() : '';
  const apiKey = typeof input.apiKey === 'string' ? input.apiKey.trim() : '';
  const model = typeof input.model === 'string' ? input.model.trim() : '';

  if (!baseUrl) throw new Error('AI 服务地址不能为空');
  if (!model) throw new Error('AI 模型不能为空');

  let url: URL;
  try {
    url = new URL(baseUrl);
  } catch {
    throw new Error('AI 服务地址格式无效');
  }
  if (!['http:', 'https:'].includes(url.protocol)) {
    throw new Error('AI 服务地址仅支持 HTTP 或 HTTPS');
  }
  if (url.username || url.password || url.search || url.hash) {
    throw new Error('AI 服务地址不能包含凭证、查询参数或片段');
  }

  return {baseUrl, apiKey, model};
}

export function classifyAiEndpoint(baseUrl: string): AiEndpointSecurity | null {
  let url: URL;
  try {
    url = new URL(baseUrl.trim());
  } catch {
    return null;
  }
  if (url.protocol === 'https:') return 'secure';
  if (url.protocol !== 'http:') return null;

  const hostname = url.hostname.toLowerCase();
  return ['localhost', '127.0.0.1', '[::1]', '::1'].includes(hostname)
    ? 'localHttp'
    : 'remoteHttp';
}

export async function loadAiServiceConfig(
  storage: AiConfigStorage = defaultStorage(),
  cipher: AiConfigCipher = defaultCipher(),
): Promise<AiServiceConfig | null> {
  const encrypted = await storage.read();
  if (!encrypted) return null;
  const plaintext = await cipher.decrypt(encrypted);
  return normalizeAiServiceConfig(JSON.parse(plaintext));
}

export async function saveAiServiceConfig(
  config: AiServiceConfig,
  storage: AiConfigStorage = defaultStorage(),
  cipher: AiConfigCipher = defaultCipher(),
): Promise<AiServiceConfig> {
  const normalized = normalizeAiServiceConfig(config);
  const encrypted = await cipher.encrypt(JSON.stringify(normalized));
  await storage.write(encrypted);
  return normalized;
}

export async function clearAiServiceConfig(
  storage: AiConfigStorage = defaultStorage(),
): Promise<void> {
  await storage.remove();
}

export async function isAiModelAvailable(
  config: AiServiceConfig,
  listModels: AiModelLister = api.cmdListAiModels,
): Promise<boolean> {
  const models = await listModels(config.baseUrl, config.apiKey.trim() || null);
  return models.some(model => model.id === config.model);
}

function defaultStorage(): AiConfigStorage {
  const store = useConfigStore();
  return {
    read: () => store.getNormalConfig('encrypted_ai_config'),
    write: value => store.saveNormalConfig('encrypted_ai_config', value),
    remove: () => store.deleteConfig('encrypted_ai_config'),
  };
}

function defaultCipher(): AiConfigCipher {
  return {
    encrypt: plaintext => api.cmdEncryptData(plaintext),
    decrypt: encrypted => api.cmdDecryptData(encrypted),
  };
}
