// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Quasar, Dark} from 'quasar';
import Component from '../VaultKeyChangeDialog.vue';
import type {KeyChangeProgress} from '../../bindings';
import * as vaultKeyChange from '../../utils/vaultKeyChange';

const api = vi.hoisted(() => ({
  cmdGetVaultKeyChangeStatus: vi.fn(async () => ({pending: false, remote: false})),
  cmdDiscardVaultKeyChangePlan: vi.fn(async () => {}),
  cmdResumeVaultKeyChange: vi.fn((_password: string, _event: {onmessage: (p: KeyChangeProgress) => void}) => new Promise<never>(() => {})),
  cmdPauseVaultKeyChange: vi.fn(async () => {}),
  cmdPlanVaultKeyChange: vi.fn(async (..._args: unknown[]) => ({planId: 'test-plan', remote: false,
    totalFiles: 0, totalBytes: 0, encryptedFiles: 0, encryptedBytes: 0, temporaryBytes: 0,
    derivationMs: 0, cryptoEstimateMs: 0, transferBytes: 0, newBootstrap: {kdf: {salt: 'test-salt'}}})),
}));
const appInfo = vi.hoisted(() => ({
  getIdentifier: vi.fn(async () => 'cn.surkaa.pad'),
  getName: vi.fn(async () => 'SurKaa Pad'),
}));
vi.mock('@tauri-apps/api/app', () => appInfo);
vi.mock('../../utils/api', () => ({default: api}));
vi.mock('@tauri-apps/api/core', () => ({Channel: class { onmessage = (_p: KeyChangeProgress) => {}; }}));
vi.mock('../../stores/timeout', () => ({useTimeoutStore: () => ({pauseAutoClose: vi.fn(() => vi.fn())})}));

let app: App | undefined;
async function mount(dark = false) {
  const host = document.createElement('div');
  document.body.append(host);
  app = createApp(Component, {modelValue: true});
  app.use(Quasar, {config: {dark}});
  app.mount(host);
  app.config.globalProperties.$q.dark.set(dark);
  await vi.advanceTimersByTimeAsync(500);
  await nextTick();
}

async function resumeChannel() {
  const input = document.querySelector<HTMLInputElement>('input[type="password"]')!;
  input.value = 'test-only-password'; input.dispatchEvent(new Event('input', {bubbles: true}));
  if (document.querySelector('.q-checkbox')?.getAttribute('aria-checked') !== 'true') {
    document.querySelector<HTMLElement>('.q-checkbox')!.click();
  }
  await nextTick();
  [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === '继续改密')!.click();
  await nextTick();
  const calls = api.cmdResumeVaultKeyChange.mock.calls;
  return calls[calls.length - 1][1];
}

async function startResume(dark = false) {
  api.cmdGetVaultKeyChangeStatus.mockResolvedValue({pending: true, remote: true});
  await mount(dark);
  return resumeChannel();
}

describe('vault key change dialog', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    api.cmdGetVaultKeyChangeStatus.mockReset().mockResolvedValue({pending: false, remote: false});
    api.cmdDiscardVaultKeyChangePlan.mockReset().mockResolvedValue(undefined);
    api.cmdResumeVaultKeyChange.mockClear();
    api.cmdPauseVaultKeyChange.mockClear();
    api.cmdPlanVaultKeyChange.mockClear();
    appInfo.getIdentifier.mockReset().mockResolvedValue('cn.surkaa.pad');
    appInfo.getName.mockReset().mockResolvedValue('SurKaa Pad');
  });
  afterEach(() => {
    app?.unmount(); app = undefined;
    Dark.set(false);
    document.body.innerHTML = '';
    vi.useRealTimers();
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  it('starts disabled until password confirmation and other-device acknowledgement are supplied', async () => {
    await mount();
    const inputs = document.querySelectorAll<HTMLInputElement>('input[type="password"]');
    const submit = [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.includes('统计数据并估算'))!;
    expect(submit.disabled).toBe(true);
    expect(document.querySelector('.device-acknowledgement')?.textContent).toContain('我确保已退出其他设备');
    const fill = (i: number, value: string) => { inputs[i].value = value; inputs[i].dispatchEvent(new Event('input', {bubbles: true})); };
    fill(0, 'old-test'); fill(1, 'new-test'); fill(2, 'different-test');
    document.querySelector<HTMLElement>('.q-checkbox')!.click();
    await nextTick();
    expect(submit.disabled).toBe(true);
    fill(2, 'new-test'); await nextTick();
    expect(submit.disabled).toBe(false);
  });

  it('uses a separately themed Teleport menu in dark mode', async () => {
    await mount(true);
    document.querySelector<HTMLElement>('.q-select')!.click();
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    const popup = document.querySelector('.key-change-memory-popup');
    expect(popup).not.toBeNull();
    expect(popup!.classList.contains('q-menu--dark')).toBe(true);
    expect(document.querySelectorAll('.q-field--dark').length).toBe(4);
  });

  it.each([false, true])('explains retained cloud history without changing the theme in dark=%s', async dark => {
    api.cmdPlanVaultKeyChange.mockResolvedValueOnce({planId: 'test-plan', remote: true,
      totalFiles: 0, totalBytes: 0, encryptedFiles: 0, encryptedBytes: 0, temporaryBytes: 0,
      derivationMs: 0, cryptoEstimateMs: 0, transferBytes: 0, newBootstrap: {kdf: {salt: 'test-salt'}}});
    vi.spyOn(vaultKeyChange, 'getLocalEncryptedConfig').mockResolvedValue({vaultVerifier: null, oss: null, ai: null});
    await mount(dark);
    const inputs = document.querySelectorAll<HTMLInputElement>('input[type="password"]');
    for (const [index, value] of ['old-test', 'new-test', 'new-test'].entries()) {
      inputs[index].value = value; inputs[index].dispatchEvent(new Event('input', {bubbles: true}));
    }
    document.querySelector<HTMLElement>('.q-checkbox')!.click(); await nextTick();
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === '统计数据并估算')!.click();
    await vi.advanceTimersByTimeAsync(0); await nextTick();
    expect(document.querySelector('[data-testid="key-change-version-history"]')).toBeNull();
    expect(document.querySelector('.notice')?.textContent).toContain('中断续改时需要新主密码');
    document.querySelector<HTMLElement>('[aria-label="改密估算说明"]')!.click();
    await vi.advanceTimersByTimeAsync(400); await nextTick();
    const notice = document.querySelector('[data-testid="key-change-version-history"]')!;
    expect(notice.textContent).toContain('不会永久清理云端历史版本');
    expect(notice.textContent).toContain('空间估算不包含');
    expect(notice.textContent).toContain('可能追溯旧密钥');
    expect(document.querySelector('[role="tooltip"]')?.textContent).toContain('5 倍保守估算');
    expect(document.querySelectorAll('.q-dialog').length).toBe(1);
    expect(document.body.classList.contains('body--dark')).toBe(dark);
  });

  it.each([false, true])('exposes the testing cost in packaged Dev and sends 1024 KiB to the plan command in dark=%s', async dark => {
    vi.stubEnv('DEV', false);
    appInfo.getIdentifier.mockResolvedValue('cn.surkaa.pad.dev');
    const local = {vaultVerifier: null, oss: null, ai: null};
    vi.spyOn(vaultKeyChange, 'getLocalEncryptedConfig').mockResolvedValue(local);
    await mount(dark);
    document.querySelector<HTMLElement>('.q-select')!.click();
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    const popup = document.querySelector('.key-change-memory-popup')!;
    expect(popup.classList.contains('q-menu--dark')).toBe(dark);
    [...popup.querySelectorAll<HTMLElement>('.q-item')].find(item => item.textContent?.includes('1 MiB'))!.click();
    await nextTick();
    expect(document.body.textContent).toContain('1 MiB 仅供开发测试');
    const inputs = document.querySelectorAll<HTMLInputElement>('input[type="password"]');
    for (const [index, value] of ['old-test', 'new-test', 'new-test'].entries()) {
      inputs[index].value = value; inputs[index].dispatchEvent(new Event('input', {bubbles: true}));
    }
    document.querySelector<HTMLElement>('.q-checkbox')!.click(); await nextTick();
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === '统计数据并估算')!.click();
    await vi.advanceTimersByTimeAsync(0); await nextTick();
    expect(api.cmdPlanVaultKeyChange).toHaveBeenCalledWith('old-test', 'new-test', 'new-test', 1024, local);
  });

  it.each([false, true])('keeps salt-only recovery guidance visible and local-copy details in help, dark=%s', async dark => {
    vi.spyOn(vaultKeyChange, 'getLocalEncryptedConfig').mockResolvedValue({vaultVerifier: null, oss: null, ai: null});
    await mount(dark);
    document.querySelector<HTMLElement>('.q-toggle')!.click(); await nextTick();
    const password = document.querySelector<HTMLInputElement>('input[type="password"]')!;
    expect(document.querySelectorAll('input[type="password"]').length).toBe(1);
    password.value = 'same-password'; password.dispatchEvent(new Event('input', {bubbles: true}));
    document.querySelector<HTMLElement>('.q-checkbox')!.click(); await nextTick();
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === '统计数据并估算')!.click();
    await vi.advanceTimersByTimeAsync(0); await nextTick();
    expect(api.cmdPlanVaultKeyChange).toHaveBeenCalledWith('same-password', 'same-password', 'same-password', null, expect.anything());
    expect(document.querySelector('.notice')?.textContent).toContain('本次只更换盐，续改时仍输入原主密码');
    expect(document.body.textContent).toContain('仅更改本地数据，云端旧副本与其他备份不会更新');
    expect(document.body.textContent).toContain('不能取消或回滚');
    expect(document.body.textContent).not.toContain('新旧密钥的副本不能直接混用');
    document.querySelector<HTMLElement>('[aria-label="改密估算说明"]')!.click();
    await vi.advanceTimersByTimeAsync(400); await nextTick();
    expect(document.querySelector('[role="tooltip"]')?.textContent).toContain('新旧密钥的副本不能直接混用');
    expect(document.querySelector('[data-testid="key-change-version-history"]')).toBeNull();
  });

  it.each([false, true])('does not offer the testing cost in production and explains the security tradeoff in dark=%s', async dark => {
    vi.stubEnv('DEV', false);
    await mount(dark);
    expect(document.body.textContent).toContain('通常越难暴力破解，解锁也更慢');
    document.querySelector<HTMLElement>('[aria-label="改密与续改说明"]')!.click();
    await vi.advanceTimersByTimeAsync(400); await nextTick();
    expect(document.querySelector('[role="tooltip"]')?.textContent).toContain('暴力破解主密码的耗时');
    expect(document.querySelector('[role="tooltip"]')?.textContent).toContain('不能替代强主密码');
    document.querySelector<HTMLElement>('.q-select')!.click();
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    const popup = document.querySelector('.key-change-memory-popup')!;
    expect(popup.textContent).not.toContain('1 MiB');
    expect(popup.textContent).toContain('保持当前内存成本');
    expect(popup.classList.contains('q-menu--dark')).toBe(dark);
  });

  it('does not offer 1 MiB when edition lookup fails for an installed app', async () => {
    vi.stubEnv('DEV', false);
    appInfo.getName.mockRejectedValue(new Error('test unavailable'));
    await mount();
    document.querySelector<HTMLElement>('.q-select')!.click();
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    expect(document.querySelector('.key-change-memory-popup')!.textContent).not.toContain('1 MiB');
  });

  it('shows new-password recovery rather than the normal old-password form when a journal is pending', async () => {
    api.cmdGetVaultKeyChangeStatus.mockResolvedValue({pending: true, remote: true});
    await mount();
    expect(document.body.textContent).toContain('续改主密码（只换盐时输入原密码）');
    expect(document.body.textContent).not.toContain('当前主密码');
    expect(document.querySelectorAll('input[type="password"]').length).toBe(1);
    expect([...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === '继续改密')?.disabled).toBe(true);
  });

  it('keeps and displays a plan-discard error instead of leaking an unhandled rejection', async () => {
    api.cmdDiscardVaultKeyChangePlan.mockRejectedValue(new Error('连接中断'));
    await mount();
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === '取消')!.click();
    await vi.advanceTimersByTimeAsync(0); await nextTick();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('连接中断');
  });

  it('renders separate quantized readback progress, clearing it for small files and resetting it for a new step', async () => {
    api.cmdGetVaultKeyChangeStatus.mockResolvedValue({pending: true, remote: true});
    await mount(true);
    const input = document.querySelector<HTMLInputElement>('input[type="password"]')!;
    input.value = 'test-only-password'; input.dispatchEvent(new Event('input', {bubbles: true}));
    document.querySelector<HTMLElement>('.q-checkbox')!.click(); await nextTick();
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === '继续改密')!.click();
    await nextTick();
    const channel = api.cmdResumeVaultKeyChange.mock.calls[0][1];
    const progress: KeyChangeProgress = {phase: 'verifying', completedFiles: 1, totalFiles: 3,
      completedBytes: 10, totalBytes: 104857600, currentFile: 'file.enc',
      fileProgress: {step: 'checkingStageContent', path: 'full', completedBytes: 10485760, totalBytes: 10485760, completedPoints: 9, totalPoints: 10}};
    channel.onmessage(progress); await nextTick();
    const section = () => document.querySelector('[data-testid="key-change-file-progress"]');
    expect(section()?.textContent).toContain('读回、解密并校验新内容');
    expect(section()?.textContent).toContain('90%');
    expect(section()?.textContent).toContain('等待本步骤确认');
    expect(section()?.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('0.9');
    channel.onmessage({...progress, fileProgress: {...progress.fileProgress!, step: 'checkingCommitted', completedBytes: 0, completedPoints: 0}});
    await nextTick();
    expect(section()?.textContent).toContain('读回校验已替换的数据');
    expect(section()?.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('0');
    channel.onmessage({...progress, fileProgress: null}); await nextTick();
    expect(section()).toBeNull();
  });

  it.each([false, true])('shows byte-based remaining time and a local completion clock in dark=%s', async dark => {
    const channel = await startResume(dark);
    const section = () => document.querySelector('[data-testid="key-change-time-estimate"]');
    expect(section()?.classList.contains('desc-text')).toBe(true);
    expect(section()?.textContent).toContain('正在按数据量估算');
    const progress: KeyChangeProgress = {phase: 'encrypting', completedFiles: 0, totalFiles: 100,
      completedBytes: 0, totalBytes: 1000, currentFile: 'file.enc', fileProgress: null};
    channel.onmessage(progress);
    await vi.advanceTimersByTimeAsync(5000); await nextTick();
    expect(section()?.textContent).toContain('正在按数据量估算');
    channel.onmessage({...progress, completedFiles: 1, completedBytes: 100}); await nextTick();
    expect(section()?.textContent).toContain('预计还需约 45 秒');
    expect(section()?.textContent).toMatch(/预计完成 \d{2}:\d{2}/);
    const estimate = section()?.textContent;
    channel.onmessage({...progress, completedFiles: 99, completedBytes: 100}); await nextTick();
    expect(section()?.textContent).toBe(estimate);
    expect(document.body.textContent).not.toContain('仅供参考');
    document.querySelector<HTMLElement>('[aria-label="预计完成时间说明"]')!.click();
    await vi.advanceTimersByTimeAsync(400); await nextTick();
    expect(document.querySelector('[role="tooltip"]')?.textContent).toContain('仅供参考');
  });

  it('estimates a large first file without falsely advancing checkpoint progress or declaring final success', async () => {
    const channel = await startResume();
    const section = () => document.querySelector('[data-testid="key-change-time-estimate"]');
    const progress: KeyChangeProgress = {phase: 'encrypting', completedFiles: 0, totalFiles: 2,
      completedBytes: 0, totalBytes: 1000 * 1024 * 1024, currentFile: 'large.enc',
      fileProgress: {step: 'writingStage', path: 'full', completedBytes: 0, totalBytes: 800 * 1024 * 1024, completedPoints: 0, totalPoints: 10}};
    channel.onmessage(progress);
    await vi.advanceTimersByTimeAsync(5000);
    channel.onmessage({...progress, fileProgress: {...progress.fileProgress!, completedBytes: 400 * 1024 * 1024, completedPoints: 5}});
    await nextTick();
    expect(section()?.textContent).toContain('预计还需约 2 分钟');
    expect(document.body.textContent).toContain('已完成并保存检查点：0 / 2');
    expect(document.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('0');
    channel.onmessage({...progress, phase: 'switchingKey', completedBytes: progress.totalBytes, completedFiles: 2, fileProgress: null});
    await nextTick();
    expect(section()?.textContent).toContain('数据处理完成，正在完成最后确认');
    expect(section()?.textContent).not.toContain('预计还需');
    expect(document.body.textContent).not.toContain('改密已完成');
  });

  it('stops after a pause and starts fresh when resuming instead of reusing the old sample', async () => {
    let rejectRun!: (reason: Error) => void;
    api.cmdResumeVaultKeyChange.mockImplementationOnce(() => new Promise<never>((_, reject) => { rejectRun = reject; }));
    const channel = await startResume();
    const progress: KeyChangeProgress = {phase: 'encrypting', completedFiles: 0, totalFiles: 100,
      completedBytes: 0, totalBytes: 1000, currentFile: 'file.enc', fileProgress: null};
    channel.onmessage(progress);
    await vi.advanceTimersByTimeAsync(5000);
    channel.onmessage({...progress, completedBytes: 100}); await nextTick();
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === '当前对象完成后暂停')!.click();
    await vi.advanceTimersByTimeAsync(0); await nextTick();
    expect(document.querySelector('[data-testid="key-change-time-estimate"]')?.textContent).toBe('将在当前对象完成后暂停');
    expect(api.cmdPauseVaultKeyChange).toHaveBeenCalledOnce();
    rejectRun(new Error('已暂停改密'));
    await vi.advanceTimersByTimeAsync(1000); await nextTick();
    expect(document.querySelector('[data-testid="key-change-time-estimate"]')).toBeNull();
    const nextChannel = await resumeChannel();
    expect(document.body.textContent).toContain('正在按数据量估算');
    channel.onmessage({...progress, completedBytes: 1000, phase: 'switchingKey'}); await nextTick();
    expect(document.body.textContent).toContain('正在按数据量估算');
    nextChannel.onmessage({...progress, completedBytes: 900});
    await vi.advanceTimersByTimeAsync(5000); await nextTick();
    expect(document.body.textContent).toContain('正在按数据量估算');
    nextChannel.onmessage({...progress, completedBytes: 950}); await nextTick();
    expect(document.body.textContent).toContain('预计还需约 5 秒');
  });

  it('disposes the ticking estimate when an active dialog is unmounted', async () => {
    await startResume();
    await vi.advanceTimersByTimeAsync(1000);
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    app!.unmount(); app = undefined;
    await vi.advanceTimersByTimeAsync(1000);
    expect(vi.getTimerCount()).toBe(0);
  });
});
