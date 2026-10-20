import {useConfigStore} from '../stores/config';
import api from './api';
import type {AiModel} from '../bindings';

export const DEFAULT_AI_BASE_URL = 'http://localhost:11434/v1';
export const CURRENT_AI_CONFIG_VERSION = 2;

interface AiModelConnectionConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
  /** 模型最大上下文 Token；空值表示不自动压缩历史。 */
  contextWindowTokens: number | null;
}

export interface AiWebSearchConfig {
  enabled: boolean;
  apiKey: string;
}

export interface AiServiceConfig extends AiModelConnectionConfig {
  /** 仅启用后才随本次请求交给后端，独立于聊天模型的服务和密钥。 */
  webSearch?: AiWebSearchConfig;
}

export interface AiServiceProfile extends AiModelConnectionConfig {
  id: string;
  name: string;
  models: AiModel[];
}

export interface AiServiceConfigSet {
  version: 2;
  activeProfileId: string | null;
  profiles: AiServiceProfile[];
  webSearch: AiWebSearchConfig;
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

function validateContextWindowTokens(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 256 || value > 10_000_000) {
    throw new Error('AI 上下文上限必须是 256 到 10,000,000 之间的整数');
  }
  return value;
}

/** 只在界面输入边界接受简写，配置、请求与持久化仍使用完整整数。 */
export function parseAiContextWindowInput(value: string | null): number | null {
  const input = value?.trim() ?? '';
  if (!input) return null;
  const match = /^(\d+(?:\.\d+)?|\.\d+)\s*([km])?$/i.exec(input);
  if (!match) throw new Error('请输入 Token 数，或使用 K / M 简写，例如 128K、1M');
  const factor = match[2]?.toUpperCase() === 'M' ? 1024 ** 2 : match[2] ? 1024 : 1;
  return validateContextWindowTokens(Number(match[1]) * factor);
}

export function normalizeAiWebSearchConfig(value: unknown): AiWebSearchConfig {
  if (value === undefined) return {enabled: false, apiKey: ''};
  if (!value || typeof value !== 'object') throw new Error('联网搜索配置格式无效');
  const input = value as Partial<AiWebSearchConfig>;
  if (typeof input.enabled !== 'boolean' || typeof input.apiKey !== 'string') {
    throw new Error('联网搜索配置格式无效');
  }
  const apiKey = input.apiKey.trim();
  if (apiKey.length > 4096 || /[\s\u0000-\u001f\u007f]/.test(apiKey)) {
    throw new Error('DeepSeek API Key 不能包含空白或控制字符');
  }
  if (input.enabled && !apiKey) throw new Error('请填写 DeepSeek 联网搜索 API Key');
  return {enabled: input.enabled, apiKey};
}

export function normalizeAiServiceConfig(value: unknown): AiServiceConfig {
  if (!value || typeof value !== 'object') {
    throw new Error('AI 服务配置格式无效');
  }
  const input = value as Partial<AiServiceConfig>;
  const baseUrl = typeof input.baseUrl === 'string' ? input.baseUrl.trim() : '';
  const apiKey = typeof input.apiKey === 'string' ? input.apiKey.trim() : '';
  const model = typeof input.model === 'string' ? input.model.trim() : '';
  const contextWindowTokens = input.contextWindowTokens === undefined || input.contextWindowTokens === null
    ? null
    : input.contextWindowTokens;

  if (!baseUrl) throw new Error('AI 服务地址不能为空');
  if (!model) throw new Error('AI 模型不能为空');
  if (contextWindowTokens !== null) validateContextWindowTokens(contextWindowTokens);

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

  return {baseUrl, apiKey, model, contextWindowTokens};
}

export function normalizeAiServiceProfile(
  value: unknown,
  fallbackId = 'default',
  fallbackName = '默认环境',
): AiServiceProfile {
  if (!value || typeof value !== 'object') {
    throw new Error('AI 环境配置格式无效');
  }
  const input = value as Partial<AiServiceProfile>;
  const config = normalizeAiServiceConfig(input);
  const id = typeof input.id === 'string' && input.id.trim() ? input.id.trim() : fallbackId;
  const name = typeof input.name === 'string' && input.name.trim() ? input.name.trim() : fallbackName;
  const models = Array.isArray(input.models)
    ? input.models.flatMap(model => {
      if (!model || typeof model !== 'object') return [];
      const candidate = model as Partial<AiModel>;
      const modelId = typeof candidate.id === 'string' ? candidate.id.trim() : '';
      if (!modelId) return [];
      return [{
        id: modelId,
        ownedBy: typeof candidate.ownedBy === 'string' ? candidate.ownedBy : null,
      }];
    })
    : [];

  return {...config, id, name, models};
}

export function normalizeAiServiceConfigSet(value: unknown): AiServiceConfigSet {
  if (!value || typeof value !== 'object') {
    throw new Error('AI 服务配置格式无效');
  }

  const input = value as Partial<Omit<AiServiceConfigSet, 'version'>> & Partial<AiServiceConfig> & {version?: number};
  if (input.version !== undefined && input.version !== 1 && input.version !== CURRENT_AI_CONFIG_VERSION) {
    throw new Error('不支持此版本的 AI 服务配置');
  }
  if ((input.version === 1 || input.version === CURRENT_AI_CONFIG_VERSION) && Array.isArray(input.profiles)) {
    const entries = input.profiles.flatMap((profile, index) => {
      try {
        return [{
          profile: normalizeAiServiceProfile(profile, `profile-${index + 1}`, `环境 ${index + 1}`),
          legacyWebTools: (profile as {webTools?: unknown}).webTools,
        }];
      } catch {
        return [];
      }
    });
    const profiles = entries.map(entry => entry.profile);
    const requestedId = typeof input.activeProfileId === 'string' ? input.activeProfileId : null;
    const active = entries.find(entry => entry.profile.id === requestedId) ?? entries[0];
    return {
      version: CURRENT_AI_CONFIG_VERSION,
      activeProfileId: active?.profile.id ?? null,
      profiles,
      // V1 曾按环境存放联网配置；仅迁移当前环境一次，此后联网配置不再跟随环境切换。
      webSearch: input.version === 1
        ? migrateLegacyWebSearch(active?.legacyWebTools, active?.profile.apiKey ?? '')
        : normalizeAiWebSearchConfig(input.webSearch),
    };
  }

  // 兼容只有一组环境的旧配置，首次读取时自动包装成配置集合。
  return {
    version: CURRENT_AI_CONFIG_VERSION,
    activeProfileId: 'default',
    profiles: [normalizeAiServiceProfile(value, 'default', '默认环境')],
    webSearch: normalizeAiWebSearchConfig(undefined),
  };
}

function migrateLegacyWebSearch(value: unknown, chatApiKey: string): AiWebSearchConfig {
  const disabled = normalizeAiWebSearchConfig(undefined);
  if (!value || typeof value !== 'object') return disabled;
  const input = value as {enabled?: unknown; baseUrl?: unknown; apiKey?: unknown};
  let endpoint: URL;
  try {
    endpoint = new URL(typeof input.baseUrl === 'string' && input.baseUrl.trim()
      ? input.baseUrl.trim() : 'https://api.deepseek.com');
  } catch {
    return disabled;
  }
  // 新实现固定使用官方服务，不能把旧代理或其他站点的凭据自动发送到不同的服务。
  if (endpoint.origin !== 'https://api.deepseek.com'
    || endpoint.username || endpoint.password || endpoint.search || endpoint.hash
    || !['/', '/v1', '/v1/'].includes(endpoint.pathname)) {
    return disabled;
  }
  const enabled = input.enabled === true;
  const apiKey = (typeof input.apiKey === 'string' ? input.apiKey.trim() : '')
    || (enabled ? chatApiKey : '');
  return normalizeAiWebSearchConfig({enabled, apiKey});
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
  const set = await loadAiServiceConfigSet(storage, cipher);
  const active = set?.profiles.find(profile => profile.id === set.activeProfileId);
  return active && set ? toAiServiceConfig(active, set.webSearch) : null;
}

export async function loadAiServiceConfigSet(
  storage: AiConfigStorage = defaultStorage(),
  cipher: AiConfigCipher = defaultCipher(),
): Promise<AiServiceConfigSet | null> {
  const encrypted = await storage.read();
  if (!encrypted) return null;
  const plaintext = await cipher.decrypt(encrypted);
  return normalizeAiServiceConfigSet(JSON.parse(plaintext));
}

export async function saveAiServiceConfig(
  config: AiServiceConfig,
  storage: AiConfigStorage = defaultStorage(),
  cipher: AiConfigCipher = defaultCipher(),
): Promise<AiServiceConfig> {
  const normalized = normalizeAiServiceConfig(config);
  const current = await loadAiServiceConfigSet(storage, cipher);
  const webSearch = config.webSearch === undefined
    ? current?.webSearch ?? normalizeAiWebSearchConfig(undefined)
    : normalizeAiWebSearchConfig(config.webSearch);
  const profile = current?.profiles.find(item => item.id === current.activeProfileId);
  const nextSet: AiServiceConfigSet = current && profile
    ? {
      ...current,
      webSearch,
      profiles: current.profiles.map(item => item.id === profile.id
        ? {...item, ...normalized}
        : item),
    }
    : {
      version: CURRENT_AI_CONFIG_VERSION,
      activeProfileId: 'default',
      profiles: [{...normalized, id: 'default', name: '默认环境', models: []}],
      webSearch,
    };
  await saveAiServiceConfigSet(nextSet, storage, cipher);
  return {...normalized, ...(webSearch.enabled ? {webSearch} : {})};
}

export async function saveAiServiceConfigSet(
  configSet: AiServiceConfigSet,
  storage: AiConfigStorage = defaultStorage(),
  cipher: AiConfigCipher = defaultCipher(),
): Promise<AiServiceConfigSet> {
  const normalized = normalizeAiServiceConfigSet(configSet);
  if (normalized.profiles.length === 0) {
    throw new Error('至少需要保留一个 AI 环境');
  }
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

function toAiServiceConfig(profile: AiServiceProfile, webSearch: AiWebSearchConfig): AiServiceConfig {
  return {
    baseUrl: profile.baseUrl,
    apiKey: profile.apiKey,
    model: profile.model,
    contextWindowTokens: profile.contextWindowTokens,
    ...(webSearch.enabled ? {webSearch: {...webSearch}} : {}),
  };
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
