import {describe, expect, it, vi} from 'vitest';
import {
  classifyAiEndpoint,
  clearAiServiceConfig,
  isAiModelAvailable,
  loadAiServiceConfig,
  loadAiServiceConfigSet,
  normalizeAiServiceConfig,
  normalizeAiServiceConfigSet,
  saveAiServiceConfig,
  saveAiServiceConfigSet,
  type AiConfigCipher,
  type AiConfigStorage,
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
  it('normalizes whitespace and optional API keys', () => {
    expect(normalizeAiServiceConfig({
      baseUrl: ' http://localhost:11434/v1 ',
      apiKey: '  ',
      model: ' qwen3:8b ',
    })).toEqual({
      baseUrl: 'http://localhost:11434/v1',
      apiKey: '',
      model: 'qwen3:8b',
    });
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
      version: 1,
      activeProfileId: 'default',
      profiles: [{
        id: 'default',
        name: '默认环境',
        baseUrl: 'https://example.com/v1',
        apiKey: 'secret-key',
        model: 'model-1',
        models: [],
      }],
    });
  });

  it('persists multiple environments, active selection and cached model lists', async () => {
    const {storage, cipher} = dependencies();
    const configSet = {
      version: 1 as const,
      activeProfileId: 'remote',
      profiles: [
        {
          id: 'local',
          name: '本地 Ollama',
          baseUrl: 'http://localhost:11434/v1',
          apiKey: '',
          model: 'qwen3:8b',
          models: [{id: 'qwen3:8b', ownedBy: 'ollama'}],
        },
        {
          id: 'remote',
          name: '远程服务',
          baseUrl: 'https://example.com/v1',
          apiKey: 'secret-key',
          model: 'model-2',
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
    });
  });

  it('clears an existing encrypted config', async () => {
    const {storage, cipher, current} = dependencies();
    await saveAiServiceConfig({
      baseUrl: 'https://example.com/v1',
      apiKey: '',
      model: 'model-1',
    }, storage, cipher);

    await clearAiServiceConfig(storage);

    expect(current()).toBeNull();
    await expect(loadAiServiceConfig(storage, cipher)).resolves.toBeNull();
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
    }, listModels)).rejects.toBe(requestError);
  });
});
