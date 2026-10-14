// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, ref, type App} from 'vue';
import {createPinia, disposePinia, type Pinia} from 'pinia';
import {Quasar, Dark, Dialog, Notify} from 'quasar';
import Component from '../VaultKeyChangeDialog.vue';
import {useTimeoutStore} from '../../stores/timeout';
import * as vaultKeyChange from '../../utils/vaultKeyChange';
import type {LocalEncryptedConfig} from '../../bindings';

const mocks = vi.hoisted(() => ({
  exit: vi.fn(async () => {}),
  failChannel: false,
  api: {
    cmdGetVaultKeyChangeStatus: vi.fn(async () => ({pending: false, remote: false})),
    cmdDiscardVaultKeyChangePlan: vi.fn(async () => {}),
    cmdPauseVaultKeyChange: vi.fn(async () => {}),
    cmdPlanVaultKeyChange: vi.fn(async (..._args: unknown[]) => ({planId: 'test-plan', remote: false,
      totalFiles: 0, totalBytes: 0, encryptedFiles: 0, encryptedBytes: 0, temporaryBytes: 0,
      derivationMs: 0, cryptoEstimateMs: 0, transferBytes: 0, newBootstrap: {kdf: {salt: 'test-salt'}}})),
    cmdStartVaultKeyChange: vi.fn((_id: string, _event: unknown) => new Promise<LocalEncryptedConfig>(() => {})),
    cmdResumeVaultKeyChange: vi.fn((_password: string, _event: unknown) => new Promise<LocalEncryptedConfig>(() => {})),
  },
}));
vi.mock('../../utils/api', () => ({default: mocks.api}));
vi.mock('@tauri-apps/plugin-process', () => ({exit: mocks.exit}));
vi.mock('@tauri-apps/api/app', () => ({getIdentifier: async () => 'cn.surkaa.pad', getName: async () => 'SurKaa Pad'}));
vi.mock('@tauri-apps/api/core', () => ({Channel: class {
  constructor() { if (mocks.failChannel) throw new Error('Channel 初始化失败'); }
  onmessage = () => {};
}}));
// 保留真实的任务计时，只手动更新显示时钟，避免两小时测试产生数十万次 RAF。
const clock = ref(Date.now());
vi.mock('@vueuse/core', async importOriginal => ({
  ...await importOriginal<typeof import('@vueuse/core')>(), useTimestamp: () => clock,
}));

function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
const HOUR = 60 * 60 * 1000;
const local: LocalEncryptedConfig = {vaultVerifier: null, oss: null, ai: null};
let app: App | undefined, pinia: Pinia;
async function advance(ms = 0) {
  vi.advanceTimersByTime(ms); clock.value = Date.now();
  await vi.advanceTimersByTimeAsync(0); await nextTick();
}
function button(label: string) {
  return [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === label)!;
}
async function mount(resume = true, dark = false) {
  mocks.api.cmdGetVaultKeyChangeStatus.mockResolvedValue({pending: resume, remote: resume});
  const host = document.createElement('div'); document.body.append(host);
  pinia = createPinia();
  app = createApp(Component, {modelValue: true});
  app.use(pinia).use(Quasar, {config: {dark}, plugins: {Dialog, Notify}});
  app.mount(host); Dark.set(dark);
  await advance(500);
  const timeout = useTimeoutStore(pinia);
  timeout.setTimeoutForCloseApp();
  return timeout;
}
async function fillCredentials(resume = true) {
  const inputs = document.querySelectorAll<HTMLInputElement>('input[type="password"]');
  for (const [index, value] of (resume ? ['new-test'] : ['old-test', 'new-test', 'new-test']).entries()) {
    inputs[index].value = value; inputs[index].dispatchEvent(new Event('input', {bubbles: true}));
  }
  document.querySelector<HTMLElement>('.q-checkbox')!.click(); await nextTick();
}
async function start(resume = true) {
  await fillCredentials(resume);
  if (!resume) { button('统计数据并估算').click(); await advance(); }
  button(resume ? '继续改密' : '确认开始改密').click(); await advance();
}

describe('key change automatic close protection with the real timeout store', () => {
  beforeEach(() => {
    vi.useFakeTimers(); clock.value = Date.now(); mocks.exit.mockClear(); mocks.failChannel = false;
    mocks.api.cmdGetVaultKeyChangeStatus.mockReset();
    mocks.api.cmdPauseVaultKeyChange.mockClear();
    vi.spyOn(vaultKeyChange, 'getLocalEncryptedConfig').mockResolvedValue(local);
    vi.spyOn(vaultKeyChange, 'finishVaultKeyChange').mockResolvedValue(false);
  });
  afterEach(() => {
    app?.unmount(); app = undefined; disposePinia(pinia);
    Dark.set(false); document.body.innerHTML = '';
    vi.restoreAllMocks(); vi.useRealTimers();
  });

  it.each([[false, false], [false, true], [true, false], [true, true]])(
    'protects a two-hour run through configuration save and final confirmation, resume=%s dark=%s', async (resume, dark) => {
      const run = deferred<LocalEncryptedConfig>(), finish = deferred<boolean>();
      const command = resume ? mocks.api.cmdResumeVaultKeyChange : mocks.api.cmdStartVaultKeyChange;
      command.mockImplementationOnce(() => run.promise);
      vi.mocked(vaultKeyChange.finishVaultKeyChange).mockReturnValueOnce(finish.promise);
      const timeout = await mount(resume, dark); await start(resume);
      expect(timeout.autoClosePaused).toBe(true);
      expect(document.querySelector('[data-testid="key-change-lock-paused"]')?.textContent).toContain('应用自动关闭已暂停');
      expect(document.body.classList.contains('body--dark')).toBe(dark);
      timeout.setTimeoutForCloseApp(); await advance(2 * HOUR);
      expect(mocks.exit).not.toHaveBeenCalled();
      run.resolve(local); await advance();
      expect(vaultKeyChange.finishVaultKeyChange).toHaveBeenCalledWith(local);
      expect(timeout.autoClosePaused).toBe(true);
      timeout.setTimeoutForCloseApp(); await advance(HOUR + 60_000);
      expect(mocks.exit).not.toHaveBeenCalled();
      finish.resolve(false); await advance();
      expect(timeout.autoClosePaused).toBe(false);
      expect(timeout.remainingStr).toBe('60:00');
      await advance(HOUR - 1); expect(mocks.exit).not.toHaveBeenCalled();
    });

  it.each(['失败', '暂停'])('restores automatic close only when the backend actually returns after %s', async outcome => {
    const run = deferred<LocalEncryptedConfig>();
    mocks.api.cmdResumeVaultKeyChange.mockImplementationOnce(() => run.promise);
    const timeout = await mount(); await start();
    if (outcome === '暂停') { button('当前对象完成后暂停').click(); await advance(); }
    expect(timeout.autoClosePaused).toBe(true);
    timeout.setTimeoutForCloseApp(); await advance(2 * HOUR);
    expect(mocks.exit).not.toHaveBeenCalled();
    run.reject(new Error(`改密${outcome}`)); await advance();
    expect(timeout.autoClosePaused).toBe(false);
    expect(timeout.remainingStr).toBe('60:00');
    expect(document.querySelector('[role="alert"]')?.textContent).toContain(`改密${outcome}`);
  });

  it.each(['成功', '失败'])('also protects a two-hour data inventory and restores the timer after %s', async outcome => {
    const inventory = deferred<Awaited<ReturnType<typeof mocks.api.cmdPlanVaultKeyChange>>>();
    mocks.api.cmdPlanVaultKeyChange.mockImplementationOnce(() => inventory.promise);
    const timeout = await mount(false); await fillCredentials(false);
    button('统计数据并估算').click(); await advance();
    expect(timeout.autoClosePaused).toBe(true);
    timeout.setTimeoutForCloseApp(); await advance(2 * HOUR);
    expect(mocks.exit).not.toHaveBeenCalled();
    if (outcome === '失败') inventory.reject(new Error('统计失败'));
    else inventory.resolve({planId: 'test-plan', remote: false,
      totalFiles: 0, totalBytes: 0, encryptedFiles: 0, encryptedBytes: 0, temporaryBytes: 0,
      derivationMs: 0, cryptoEstimateMs: 0, transferBytes: 0, newBootstrap: {kdf: {salt: 'test-salt'}}});
    await advance();
    expect(timeout.autoClosePaused).toBe(false);
    expect(timeout.remainingStr).toBe('60:00');
    if (outcome === '失败') expect(document.querySelector('[role="alert"]')?.textContent).toContain('统计失败');
    else expect(button('确认开始改密')).toBeTruthy();
  });

  it('keeps protecting the backend task if its dialog is unmounted', async () => {
    const run = deferred<LocalEncryptedConfig>();
    mocks.api.cmdResumeVaultKeyChange.mockImplementationOnce(() => run.promise);
    const timeout = await mount(); await start();
    app!.unmount(); app = undefined;
    timeout.setTimeoutForCloseApp(); await advance(2 * HOUR);
    expect(timeout.autoClosePaused).toBe(true);
    expect(mocks.exit).not.toHaveBeenCalled();
    run.resolve(local); await advance();
    expect(timeout.autoClosePaused).toBe(false);
  });

  it('releases protection even if stream channel setup fails before calling the backend', async () => {
    const timeout = await mount(); mocks.failChannel = true; await start();
    expect(timeout.autoClosePaused).toBe(false);
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('Channel 初始化失败');
  });
});
