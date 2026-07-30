import {describe, expect, it, vi} from 'vitest';
import {
  classifyAiEndpoint,
  clearAiServiceConfig,
  loadAiServiceConfig,
  normalizeAiServiceConfig,
  saveAiServiceConfig,
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
});
