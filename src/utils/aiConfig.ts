import {useConfigStore} from '../stores/config';
import api from './api';
import type {AiModel} from '../bindings';

export const DEFAULT_AI_BASE_URL = 'http://localhost:11434/v1';

export interface AiServiceConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
}

export interface AiServiceProfile extends AiServiceConfig {
  id: string;
  name: string;
  models: AiModel[];
}

export interface AiServiceConfigSet {
  version: 1;
  activeProfileId: string | null;
  profiles: AiServiceProfile[];
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

  const input = value as Partial<AiServiceConfigSet> & Partial<AiServiceConfig>;
  if (input.version === 1 && Array.isArray(input.profiles)) {
    const profiles = input.profiles.flatMap((profile, index) => {
      try {
        return [normalizeAiServiceProfile(profile, `profile-${index + 1}`, `环境 ${index + 1}`)];
      } catch {
        return [];
      }
    });
    const requestedId = typeof input.activeProfileId === 'string' ? input.activeProfileId : null;
    return {
      version: 1,
      activeProfileId: profiles.some(profile => profile.id === requestedId)
        ? requestedId
        : profiles[0]?.id ?? null,
      profiles,
    };
  }

  // 兼容只有一组环境的旧配置，首次读取时自动包装成配置集合。
  return {
    version: 1,
    activeProfileId: 'default',
    profiles: [normalizeAiServiceProfile(value, 'default', '默认环境')],
  };
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
  return active ? toAiServiceConfig(active) : null;
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
  const profile = current?.profiles.find(item => item.id === current.activeProfileId);
  const nextSet: AiServiceConfigSet = current && profile
    ? {
      ...current,
      profiles: current.profiles.map(item => item.id === profile.id
        ? {...item, ...normalized}
        : item),
    }
    : {
      version: 1,
      activeProfileId: 'default',
      profiles: [{...normalized, id: 'default', name: '默认环境', models: []}],
    };
  await saveAiServiceConfigSet(nextSet, storage, cipher);
  return normalized;
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

function toAiServiceConfig(profile: AiServiceProfile): AiServiceConfig {
  return {
    baseUrl: profile.baseUrl,
    apiKey: profile.apiKey,
    model: profile.model,
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
