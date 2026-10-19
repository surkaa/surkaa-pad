// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Notify, Quasar} from 'quasar';
import Component from '../AiSettings.vue';
import {CURRENT_AI_CONFIG_VERSION, type AiServiceConfigSet} from '../../../utils/aiConfig';

const {loadConfig, saveConfig, testSearch} = vi.hoisted(() => ({
  loadConfig: vi.fn(), saveConfig: vi.fn(), testSearch: vi.fn(),
}));
vi.mock('../../../utils/aiConfig', async original => ({
  ...await original<typeof import('../../../utils/aiConfig')>(),
  loadAiServiceConfigSet: loadConfig, saveAiServiceConfigSet: saveConfig,
}));
vi.mock('../../../utils/api', () => ({default: {cmdTestAiWebSearch: testSearch}}));

let app: App | undefined;
function savedConfig(): AiServiceConfigSet {
  return {version: CURRENT_AI_CONFIG_VERSION, activeProfileId: 'local', webSearch: {enabled: false, apiKey: ''}, profiles: [{
    id: 'local', name: '本地 Ollama', baseUrl: 'http://localhost:11434/v1', apiKey: 'model-secret',
    model: 'qwen', contextWindowTokens: null, models: [{id: 'qwen', ownedBy: null}],
  }]};
}
async function settle() { await vi.advanceTimersByTimeAsync(500); await nextTick(); }
async function open(dark = false) {
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(Component); app.use(Quasar, {plugins: {Notify}}); app.mount(host);
  Dark.set(dark); await settle();
  document.querySelector<HTMLElement>('.settings-item')!.click(); await settle();
}
function button(label: string): HTMLElement {
  const result = [...document.querySelectorAll<HTMLElement>('.q-btn')].find(node => node.textContent?.trim() === label);
  expect(result, label).toBeDefined(); return result!;
}
async function enable() { document.querySelector<HTMLElement>('.web-search-toggle')!.click(); await settle(); }
async function enterKey(value: string) {
  const input = document.querySelector<HTMLInputElement>('.web-search-key input')!;
  input.value = value; input.dispatchEvent(new Event('input', {bubbles: true})); await settle();
}
beforeEach(() => {
  vi.useFakeTimers(); loadConfig.mockReset().mockResolvedValue(savedConfig());
  saveConfig.mockReset().mockImplementation(async config => config);
  testSearch.mockReset().mockResolvedValue(null);
});
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers();
});

describe('AI web search settings', () => {
  it.each([false, true])('keeps search opt-in and themes the password field (dark=%s)', async dark => {
    await open(dark);
    const field = document.querySelector<HTMLElement>('.web-search-key')!;
    const input = field.querySelector<HTMLInputElement>('input')!;
    expect(field.classList.contains('q-field--dark')).toBe(dark);
    expect(input.type).toBe('password'); expect(input.disabled).toBe(true);
    expect(testSearch).not.toHaveBeenCalled();
    expect(document.querySelector('.web-search-notice')?.textContent).toContain('按请求计费');
    expect(document.querySelector('.web-search-notice')?.textContent).toContain('按需提供的原始问题');
    expect(document.querySelector('.web-search-notice')?.textContent).toContain('模型可选择搜索条数及是否直接获取正文');
    const body = document.querySelector('.ai-config-body')!;
    expect(body.contains(button('保存'))).toBe(false);
    await enable(); expect(input.disabled).toBe(false);
  });

  it('saves one encrypted-config payload with a separate search key', async () => {
    await open(); await enable(); await enterKey(' web-secret ');
    button('保存').click(); await settle();
    expect(saveConfig).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
      version: CURRENT_AI_CONFIG_VERSION, webSearch: {enabled: true, apiKey: 'web-secret'},
      profiles: [expect.objectContaining({apiKey: 'model-secret'})],
    }));
    expect(testSearch).not.toHaveBeenCalled();
  });

  it('does not save enabled search without a key or make an automatic paid request', async () => {
    await open(); await enable(); button('保存').click(); await settle();
    expect(saveConfig).not.toHaveBeenCalled(); expect(testSearch).not.toHaveBeenCalled();
  });

  it('only runs a paid connection test after an explicit click and uses the web key', async () => {
    await open(); await enable(); await enterKey(' web-secret ');
    expect(testSearch).not.toHaveBeenCalled();
    button('测试搜索（调用 1 次接口）').click(); await settle();
    expect(testSearch).toHaveBeenCalledExactlyOnceWith({apiKey: 'web-secret'});
    expect(document.querySelector('.web-search-test')?.classList.contains('q-btn--loading')).toBe(false);
  });

  it('keeps the global web draft when a different model environment is added', async () => {
    await open(); await enable(); await enterKey('web-secret');
    document.querySelector<HTMLElement>('[aria-label="新增 AI 环境"]')!.click(); await settle();
    expect(document.querySelector<HTMLInputElement>('.web-search-key input')!.value).toBe('web-secret');
    expect(testSearch).not.toHaveBeenCalled();
  });
});
