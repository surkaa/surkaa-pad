import {describe, expect, it, vi} from 'vitest';
import {
  classifyAiEndpoint,
  clearAiServiceConfig,
  CURRENT_AI_CONFIG_VERSION,
  isAiModelAvailable,
  loadAiServiceConfig,
  loadAiServiceConfigSet,
  normalizeAiServiceConfig,
  normalizeAiServiceConfigSet,
  normalizeAiWebSearchConfig,
  parseAiContextWindowInput,
  saveAiServiceConfig,
  saveAiServiceConfigSet,
  type AiConfigCipher,
  type AiConfigStorage,
  type AiServiceConfigSet,
} from '../aiConfig';

function dependencies() {
  let stored: number[] | null = null;
  const storage: AiConfigStorage = {
    read: vi.fn(async () => stored),
    write: vi.fn(async value => {
      stored = value;
    }),
    remove: vi.fn(async () => {
      stored = null;
    }),
  };
  const cipher: AiConfigCipher = {
    encrypt: vi.fn(async plaintext => Array.from(new TextEncoder().encode(plaintext)).reverse()),
    decrypt: vi.fn(async encrypted => new TextDecoder().decode(Uint8Array.from([...encrypted].reverse()))),
  };
  return {storage, cipher, current: () => stored};
}

describe('AI service config', () => {
  it.each([
    ['', null], ['  ', null], [null, null], ['1048576', 1048576], ['1M', 1048576],
    [' 128k ', 131072], ['1.5 M', 1572864], ['.5K', 512], ['256', 256], ['10000000', 10000000],
  ])('parses the context input %s into %s tokens', (input, tokens) => {
    expect(parseAiContextWindowInput(input)).toBe(tokens);
  });

  it.each(['0', '255', '-1', '1.5', '10M', '10000001', '1.23K', 'NaN', 'Infinity', '1e6', '1MB', '1Mbad'])('rejects an invalid context input %s', input => {
    expect(() => parseAiContextWindowInput(input)).toThrow();
  });

  it('normalizes whitespace and optional API keys', () => {
    expect(normalizeAiServiceConfig({
      baseUrl: ' http://localhost:11434/v1 ',
      apiKey: '  ',
      model: ' qwen3:8b ',
    })).toEqual({
      baseUrl: 'http://localhost:11434/v1',
      apiKey: '',
      model: 'qwen3:8b',
      contextWindowTokens: null,
    });
  });

  it('preserves a valid context window and rejects unsafe values', () => {
    expect(normalizeAiServiceConfig({
      baseUrl: 'http://localhost:11434/v1',
      apiKey: '',
      model: 'qwen3:8b',
      contextWindowTokens: 32_768,
    }).contextWindowTokens).toBe(32_768);
    for (const contextWindowTokens of [0, 255, 1.5, 10_000_001]) {
      expect(() => normalizeAiServiceConfig({
        baseUrl: 'http://localhost:11434/v1',
        apiKey: '',
        model: 'qwen3:8b',
        contextWindowTokens,
      })).toThrow();
    }
  });

  it('rejects invalid or credential-bearing URLs', () => {
    for (const baseUrl of [
      'not-a-url',
      'ftp://example.com/v1',
      'https://user:password@example.com/v1',
      'https://example.com/v1?token=secret',
    ]) {
      expect(() => normalizeAiServiceConfig({baseUrl, apiKey: '', model: 'model'})).toThrow();
    }
  });

  it('distinguishes local HTTP, remote HTTP and HTTPS endpoints', () => {
    expect(classifyAiEndpoint('http://localhost:11434/v1')).toBe('localHttp');
    expect(classifyAiEndpoint('http://192.168.1.10:11434/v1')).toBe('remoteHttp');
    expect(classifyAiEndpoint('https://example.com/v1')).toBe('secure');
    expect(classifyAiEndpoint('invalid')).toBeNull();
  });

  it('persists only encrypted bytes and restores the normalized config', async () => {
    const {storage, cipher, current} = dependencies();
    const config = {
      baseUrl: 'https://example.com/v1',
      apiKey: 'secret-key',
      model: 'model-1',
      contextWindowTokens: null,
    };

    await saveAiServiceConfig(config, storage, cipher);

    expect(current()).toEqual(expect.any(Array));
    expect(JSON.stringify(current())).not.toContain('secret-key');
    await expect(loadAiServiceConfig(storage, cipher)).resolves.toEqual(config);
  });

  it('wraps a legacy single environment into a profile set', () => {
    expect(normalizeAiServiceConfigSet({
      baseUrl: 'https://example.com/v1',
      apiKey: 'secret-key',
      model: 'model-1',
    })).toEqual({
      version: CURRENT_AI_CONFIG_VERSION,
      activeProfileId: 'default',
      webSearch: {enabled: false, apiKey: ''},
      profiles: [{
        id: 'default',
        name: '默认环境',
        baseUrl: 'https://example.com/v1',
        apiKey: 'secret-key',
        model: 'model-1',
        contextWindowTokens: null,
        models: [],
      }],
    });
  });

  it('persists multiple environments, active selection and cached model lists', async () => {
    const {storage, cipher} = dependencies();
    const configSet: AiServiceConfigSet = {
      version: CURRENT_AI_CONFIG_VERSION,
      activeProfileId: 'remote',
      webSearch: {enabled: false, apiKey: ''},
      profiles: [
        {
          id: 'local',
          name: '本地 Ollama',
          baseUrl: 'http://localhost:11434/v1',
          apiKey: '',
          model: 'qwen3:8b',
          contextWindowTokens: null,
          models: [{id: 'qwen3:8b', ownedBy: 'ollama'}],
        },
        {
          id: 'remote',
          name: '远程服务',
          baseUrl: 'https://example.com/v1',
          apiKey: 'secret-key',
          model: 'model-2',
          contextWindowTokens: null,
          models: [{id: 'model-2', ownedBy: 'provider'}],
        },
      ],
    };

    await saveAiServiceConfigSet(configSet, storage, cipher);

    expect(JSON.stringify(await storage.read())).not.toContain('secret-key');
    await expect(loadAiServiceConfigSet(storage, cipher)).resolves.toEqual(configSet);
    await expect(loadAiServiceConfig(storage, cipher)).resolves.toEqual({
      baseUrl: 'https://example.com/v1',
      apiKey: 'secret-key',
      model: 'model-2',
      contextWindowTokens: null,
    });
  });

  it('clears an existing encrypted config', async () => {
    const {storage, cipher, current} = dependencies();
    await saveAiServiceConfig({
      baseUrl: 'https://example.com/v1',
      apiKey: '',
      model: 'model-1',
      contextWindowTokens: null,
    }, storage, cipher);

    await clearAiServiceConfig(storage);

    expect(current()).toBeNull();
    await expect(loadAiServiceConfig(storage, cipher)).resolves.toBeNull();
  });

  it('migrates v1 profiles without enabling search or changing cached models', () => {
    const legacy = {version: 1, activeProfileId: 'old', profiles: [{
      id: 'old', name: '旧环境', baseUrl: 'http://localhost:11434/v1', apiKey: '', model: 'qwen',
      contextWindowTokens: 4096, models: [{id: 'qwen', ownedBy: 'ollama'}],
    }]};
    const migrated = normalizeAiServiceConfigSet(legacy);
    expect(migrated.version).toBe(CURRENT_AI_CONFIG_VERSION);
    expect(migrated.profiles).toEqual(legacy.profiles);
    expect(migrated.webSearch).toEqual({enabled: false, apiKey: ''});
    expect(() => normalizeAiServiceConfigSet({...legacy, version: 99})).toThrow('不支持');
  });

  it('validates search opt-in and rejects header-unsafe keys without exposing them', () => {
    expect(normalizeAiWebSearchConfig(undefined)).toEqual({enabled: false, apiKey: ''});
    expect(normalizeAiWebSearchConfig({enabled: true, apiKey: ' sk-web '})).toEqual({enabled: true, apiKey: 'sk-web'});
    for (const value of [null, {}, {enabled: true, apiKey: ''}, {enabled: 'yes', apiKey: 'key'},
      {enabled: true, apiKey: 'secret\nkey'}, {enabled: false, apiKey: 'secret key'}, {enabled: true, apiKey: 'k'.repeat(4097)}]) {
      expect(() => normalizeAiWebSearchConfig(value)).toThrow();
    }
  });

  it('migrates only the active legacy DeepSeek configuration into global settings', async () => {
    const legacyProfile = (id: string, apiKey: string) => ({
      id, name: id, baseUrl: 'http://localhost:11434/v1', apiKey: 'chat-key', model: 'qwen',
      webTools: {enabled: true, baseUrl: 'https://api.deepseek.com/v1/', apiKey, maxResults: 8},
    });
    const legacy = {version: 1, activeProfileId: 'second', profiles: [
      legacyProfile('first', 'first-web-key'), legacyProfile('second', ' second-web-key '),
    ]};
    const migrated = normalizeAiServiceConfigSet(legacy);
    expect(migrated.webSearch).toEqual({enabled: true, apiKey: 'second-web-key'});
    expect(migrated.profiles.every(profile => !('webTools' in profile))).toBe(true);
    const {storage, cipher, current} = dependencies();
    await saveAiServiceConfigSet({...migrated, activeProfileId: 'first'}, storage, cipher);
    expect((await loadAiServiceConfig(storage, cipher))?.webSearch).toEqual(migrated.webSearch);
    expect(JSON.stringify(current())).not.toContain('second-web-key');
  });

  it('makes a previously reused DeepSeek chat key independent after migration', () => {
    const profile = {id: 'old', name: '旧环境', baseUrl: 'https://api.deepseek.com/v1',
      apiKey: 'chat-key', model: 'deepseek-chat',
      webTools: {enabled: true, baseUrl: 'https://api.deepseek.com', apiKey: ''}};
    const migrated = normalizeAiServiceConfigSet({version: 1, activeProfileId: 'old', profiles: [profile]});
    expect(migrated.webSearch).toEqual({enabled: true, apiKey: 'chat-key'});
    migrated.profiles[0].apiKey = 'new-chat-key';
    expect(normalizeAiServiceConfigSet(migrated).webSearch.apiKey).toBe('chat-key');
    expect(normalizeAiServiceConfigSet({version: 1, activeProfileId: 'old', profiles: [{
      ...profile, webTools: {...profile.webTools, enabled: false, apiKey: 'saved-web-key'},
    }]}).webSearch).toEqual({enabled: false, apiKey: 'saved-web-key'});
  });

  it('does not redirect custom-endpoint credentials to DeepSeek during migration', () => {
    for (const baseUrl of ['https://proxy.example.com/v1', 'http://api.deepseek.com',
      'https://api.deepseek.com:8443', 'https://api.deepseek.com/proxy',
      'https://user:password@api.deepseek.com', 'https://api.deepseek.com?token=x', 'invalid']) {
      const legacy = {version: 1, activeProfileId: 'old', profiles: [{
        id: 'old', name: '旧环境', baseUrl: 'http://localhost:11434/v1', apiKey: 'chat-key', model: 'qwen',
        webTools: {enabled: true, baseUrl, apiKey: 'proxy-key'},
      }]};
      const migrated = normalizeAiServiceConfigSet(legacy);
      expect(migrated.profiles).toHaveLength(1);
      expect(migrated.webSearch).toEqual({enabled: false, apiKey: ''});
    }
  });

  it('encrypts the independent search key and preserves it when updating the model', async () => {
    const {storage, cipher, current} = dependencies();
    const webSearch = {enabled: true, apiKey: 'independent-web-secret'};
    const model = {baseUrl: 'http://localhost:11434/v1', apiKey: '', model: 'qwen', contextWindowTokens: null};
    await saveAiServiceConfig({...model, webSearch}, storage, cipher);
    expect(JSON.stringify(current())).not.toContain(webSearch.apiKey);
    await expect(loadAiServiceConfig(storage, cipher)).resolves.toEqual({...model, webSearch});
    await saveAiServiceConfig({...model, model: 'new-model'}, storage, cipher);
    expect((await loadAiServiceConfigSet(storage, cipher))?.webSearch).toEqual(webSearch);
    expect((await loadAiServiceConfig(storage, cipher))?.webSearch).toEqual(webSearch);
  });

  it('keeps a disabled key encrypted but does not send it to the agent', async () => {
    const {storage, cipher} = dependencies();
    await saveAiServiceConfig({baseUrl: 'http://localhost:11434/v1', apiKey: '', model: 'qwen', contextWindowTokens: null,
      webSearch: {enabled: false, apiKey: 'disabled-web-secret'}}, storage, cipher);
    expect((await loadAiServiceConfigSet(storage, cipher))?.webSearch.apiKey).toBe('disabled-web-secret');
    expect(await loadAiServiceConfig(storage, cipher)).not.toHaveProperty('webSearch');
  });

  it('checks the configured model against the latest model list', async () => {
    const listModels = vi.fn(async () => [
      {id: 'model-1', ownedBy: null},
      {id: 'model-2', ownedBy: 'provider'},
    ]);
    const config = {
      baseUrl: 'https://example.com/v1',
      apiKey: ' secret-key ',
      model: 'model-2',
      contextWindowTokens: null,
    };

    await expect(isAiModelAvailable(config, listModels)).resolves.toBe(true);
    expect(listModels).toHaveBeenCalledWith('https://example.com/v1', 'secret-key');
  });

  it('reports a missing model and omits an empty API key', async () => {
    const listModels = vi.fn(async () => [{id: 'other-model', ownedBy: null}]);
    const config = {
      baseUrl: 'http://localhost:11434/v1',
      apiKey: '  ',
      model: 'missing-model',
      contextWindowTokens: null,
    };

    await expect(isAiModelAvailable(config, listModels)).resolves.toBe(false);
    expect(listModels).toHaveBeenCalledWith('http://localhost:11434/v1', null);
  });

  it('preserves model-list request errors for the caller to display', async () => {
    const requestError = new Error('service unavailable');
    const listModels = vi.fn(async () => {
      throw requestError;
    });

    await expect(isAiModelAvailable({
      baseUrl: 'https://example.com/v1',
      apiKey: '',
      model: 'model-1',
      contextWindowTokens: null,
    }, listModels)).rejects.toBe(requestError);
  });
});
