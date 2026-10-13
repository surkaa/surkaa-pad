// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import Component from '../VaultKdfMemoryField.vue';
import FirstTimeForm from '../FirstTimeUnlockForm.vue';
import RemoteSetupForm from '../RemoteSetupForm.vue';

const appInfo = vi.hoisted(() => ({
  getIdentifier: vi.fn(async () => 'cn.surkaa.pad'),
  getName: vi.fn(async () => 'SurKaa Pad'),
}));
vi.mock('@tauri-apps/api/app', () => appInfo);

let app: App | undefined;
async function mount(dark: boolean, props: Record<string, unknown> = {}, component: Parameters<typeof createApp>[0] = Component) {
  const host = document.createElement('div');
  document.body.append(host);
  app = createApp(component, {modelValue: 256 * 1024, loading: false, ...props});
  app.use(Quasar, {config: {dark}});
  app.mount(host);
  app.config.globalProperties.$q.dark.set(dark);
  await vi.advanceTimersByTimeAsync(500);
  await nextTick();
}
async function openMenu() {
  document.querySelector<HTMLElement>('.q-select')!.click();
  await vi.advanceTimersByTimeAsync(500); await nextTick();
  return document.querySelector('.kdf-memory-select-popup')!;
}

describe('vault KDF memory choices', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubEnv('DEV', false); // 模拟安装包，而不是 Vite 开发服务器。
    appInfo.getIdentifier.mockReset().mockResolvedValue('cn.surkaa.pad');
    appInfo.getName.mockReset().mockResolvedValue('SurKaa Pad');
  });
  afterEach(() => {
    app?.unmount(); app = undefined;
    Dark.set(false);
    document.body.innerHTML = '';
    vi.unstubAllEnvs();
    vi.useRealTimers();
  });

  it.each([false, true])('allows 1 MiB in a packaged Windows Dev app in dark=%s', async dark => {
    appInfo.getIdentifier.mockResolvedValue('cn.surkaa.pad.dev');
    const selected = vi.fn();
    await mount(dark, {'onUpdate:modelValue': selected});
    const menu = await openMenu();
    expect(menu.classList.contains('q-menu--dark')).toBe(dark);
    [...menu.querySelectorAll<HTMLElement>('.q-item')].find(item => item.textContent?.includes('1 MiB'))!.click();
    await nextTick();
    expect(selected).toHaveBeenCalledWith(1024);
  });

  it.each([false, true])('keeps production options and shows the brute-force cost hint in dark=%s', async dark => {
    await mount(dark);
    expect(document.querySelector('.field-hint')?.textContent).toContain('暴力破解主密码的耗时');
    expect(document.querySelector('.field-hint')?.textContent).toContain('本机解锁耗时和内存占用');
    const menu = await openMenu();
    expect(menu.textContent).not.toContain('1 MiB');
    expect([...menu.querySelectorAll('.q-item')].map(item => item.textContent)).toEqual([
      '64 MiB · 兼容', '128 MiB · 均衡', '256 MiB · 更强',
    ]);
    expect(menu.classList.contains('q-menu--dark')).toBe(dark);
  });

  it('detects Android Dev by product name despite its shared application identifier', async () => {
    appInfo.getName.mockResolvedValue('SurKaa Pad (Dev)');
    await mount(true, {modelValue: 1024});
    expect(document.body.textContent).toContain('1 MiB 仅供开发测试');
    expect((await openMenu()).textContent).toContain('1 MiB · 开发测试');
  });

  it('keeps the security hint alongside the cloud-parameter rule', async () => {
    await mount(false, {remoteSetup: true});
    expect(document.body.textContent).toContain('暴力破解主密码');
    expect(document.body.textContent).toContain('已有 Vault 会采用云端参数');
  });

  it('does not expose 1 MiB when native edition detection fails', async () => {
    appInfo.getIdentifier.mockRejectedValue(new Error('test unavailable'));
    await mount(false);
    expect((await openMenu()).textContent).not.toContain('1 MiB');
  });

  it('keeps the inexpensive option in live development without reading native metadata', async () => {
    vi.stubEnv('DEV', true);
    await mount(false);
    expect((await openMenu()).textContent).toContain('1 MiB');
    expect(appInfo.getIdentifier).not.toHaveBeenCalled();
    expect(appInfo.getName).not.toHaveBeenCalled();
  });

  it.each([false, true])('passes the numeric 1 MiB choice through the first-time local/cloud form remote=%s', async remote => {
    appInfo.getIdentifier.mockResolvedValue('cn.surkaa.pad.dev');
    const selected = vi.fn();
    await mount(false, {masterPassword: 'test', confirmPassword: 'test', memoryCostKib: 256 * 1024,
      ossConfig: {akid: '', aks: '', bucket: '', endpoint: ''}, quickConfig: '',
      'onUpdate:memoryCostKib': selected}, remote ? RemoteSetupForm : FirstTimeForm);
    const menu = await openMenu();
    [...menu.querySelectorAll<HTMLElement>('.q-item')].find(item => item.textContent?.includes('1 MiB'))!.click();
    await nextTick();
    expect(selected).toHaveBeenCalledWith(1024);
  });
});
