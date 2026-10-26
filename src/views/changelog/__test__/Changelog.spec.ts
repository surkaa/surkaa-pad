// @vitest-environment happy-dom
import {afterAll, afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, type App} from 'vue';
import {createPinia, disposePinia, type Pinia} from 'pinia';
import {createMemoryHistory, createRouter, RouterView, type Router} from 'vue-router';
import {Dark, Quasar} from 'quasar';
import appRouter from '../../../router';
import data from '../../../data/changelog.json';
import {formatChangelogTimestamp} from '../../../utils/changelog';

vi.mock('../../unlock/Unlock.vue', () => ({default: {render: () => null}}));
vi.mock('../../diary-list/DiaryList.vue', () => ({default: {render: () => null}}));
// 只替换设置页里的其他模块，实际使用关于/更新日志两个同级入口及主路由配置。
vi.mock('../../settings/Settings.vue', async () => ({default: (await import('../../settings/AboutSettings.vue')).default}));
const about = vi.hoisted(() => ({load: vi.fn()}));
const environment = vi.hoisted(() => ({getIdentifier: vi.fn(), getName: vi.fn(), platform: vi.fn()}));
vi.mock('@tauri-apps/api/app', () => ({getIdentifier: environment.getIdentifier, getName: environment.getName}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: environment.platform}));
vi.mock('../../../utils/aboutInfo', () => ({
  loadAboutInfo: about.load,
  formatAboutSummary: (info: {appVersion: string}) => `版本 ${info.appVersion}`,
}));

let app: App | undefined;
let router: Router;
let pinia: Pinia;
let host: HTMLElement;
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick(); }
async function mount(path = '/changelog', dark = false) {
  router = createRouter({history: createMemoryHistory(), routes: appRouter.options.routes});
  pinia = createPinia();
  app = createApp({render: () => h(RouterView)});
  app.use(pinia); app.use(Quasar); app.use(router);
  await router.push(path); await router.isReady();
  host = document.createElement('div'); document.body.append(host);
  Dark.set(dark); app.mount(host); await settle();
}
function versions() { return Array.from(host.querySelectorAll('.changelog-entry'), node => node.getAttribute('data-version')); }
function nextNavigation() {
  return new Promise<void>(resolve => {
    const stop = router.afterEach(() => { stop(); resolve(); });
  });
}
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks();
  vi.stubEnv('DEV', false);
  environment.getIdentifier.mockReset().mockResolvedValue('cn.surkaa.pad');
  environment.getName.mockReset().mockResolvedValue('SurKaa Pad');
  environment.platform.mockReset().mockReturnValue('windows');
  about.load.mockResolvedValue({
    appName: 'SurKaa Pad', appVersion: '0.8.16', gitCommit: '12345678', identifier: 'cn.surkaa.pad',
    tauriVersion: '2', platform: 'Windows', architecture: 'x86_64', osVersion: '11',
  });
});
afterEach(() => {
  app?.unmount(); app = undefined; router?.options.history.destroy(); if (pinia) disposePinia(pinia);
  document.body.innerHTML = ''; Dark.set(false); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllEnvs();
});
afterAll(() => appRouter.options.history.destroy());

describe.each([false, true])('独立更新日志页面（深色=%s）', dark => {
  it('设置底部入口与关于入口并列，进入独立页面并可返回设置', async () => {
    await mount('/settings', dark);
    const list = host.querySelector('.settings-group > .pad-card')!;
    const entry = list.querySelector<HTMLElement>('[aria-label="更新日志"]')!;
    expect(entry).not.toBeNull();
    expect(entry.parentElement).toBe(list);
    expect(list.querySelectorAll(':scope > .settings-item')).toHaveLength(2);
    expect(list.lastElementChild).toBe(entry);
    expect(entry.getAttribute('href')).toBe('/changelog');
    const entering = nextNavigation();
    entry.click(); await entering; await settle();
    expect(router.currentRoute.value.name).toBe('Changelog');
    expect(router.currentRoute.value.meta.depth).toBe(3);
    expect(host.querySelector('.header-global')?.textContent?.trim()).toBe('更新日志');
    expect(host.querySelector('.app-footer')).toBeNull();
    expect(document.querySelector('.about-dialog')).toBeNull();
    expect(host.querySelector('.changelog-page')).not.toBeNull();
    const returning = nextNavigation();
    router.back(); await returning; await settle();
    expect(router.currentRoute.value.name).toBe('Settings');
    expect(host.querySelector('[aria-label="更新日志"]')).not.toBeNull();
  });

  it('正式包只显示全部正式版，包括早期版本，按JSON顺序展示纯文本及本地日期', async () => {
    await mount('/changelog', dark);
    expect(versions()).toEqual(data.stable.map(record => record.version));
    expect(host.querySelector('[data-version="0.4.9"]')).not.toBeNull();
    expect(host.querySelector('[data-version="0.0.1"]')).not.toBeNull();
    const descriptions = Array.from(host.querySelectorAll('.entry-description'), node => node.textContent);
    expect(descriptions).toEqual(data.stable.map(record => record.description));
    const first = host.querySelector('.changelog-entry')!;
    expect(first.querySelector('time')?.textContent).toBe(formatChangelogTimestamp(data.stable[0].updatedAt));
    expect(first.querySelector('time')?.getAttribute('datetime')).toBe(new Date(data.stable[0].updatedAt).toISOString());
    expect(first.querySelector('.entry-latest')?.textContent).toBe('最新记录');
    expect(host.querySelectorAll('.entry-latest')).toHaveLength(1);
    expect(host.querySelector('.changelog-groups')).toBeNull();
    expect(host.querySelector('.changelog-summary')?.textContent).toContain(`正式版 · ${data.stable.length} 个版本`);
    expect(host.textContent).not.toMatch(/预览版|Windows Dev|Android Dev/);
    expect(about.load).not.toHaveBeenCalled(); // 仅需要应用版本标识，不读取完整的关于信息。
    expect(document.body.classList.contains('body--dark')).toBe(dark);
  });

  it.each([
    ['windows', false, 'cn.surkaa.pad', 'SurKaa Pad', 'stable', '正式版'],
    ['android', false, 'cn.surkaa.pad', 'SurKaa Pad', 'stable', '正式版'],
    ['windows', false, 'cn.surkaa.pad.dev', 'SurKaa Pad (Dev)', 'windowsDev', 'Windows Dev'],
    ['android', false, 'cn.surkaa.pad', 'SurKaa Pad (Dev)', 'androidDev', 'Android Dev'],
    ['windows', true, 'cn.surkaa.pad', 'SurKaa Pad', 'windowsDev', 'Windows Dev'],
    ['android', true, 'cn.surkaa.pad', 'SurKaa Pad', 'androidDev', 'Android Dev'],
  ] as const)('%s 开发运行=%s 的包仅显示自身版本记录', async (currentPlatform, frontendDev, identifier, name, group, label) => {
    vi.stubEnv('DEV', frontendDev);
    environment.getIdentifier.mockResolvedValue(identifier);
    environment.getName.mockResolvedValue(name);
    environment.platform.mockReturnValue(currentPlatform);
    await mount('/changelog', dark);
    expect(versions()).toEqual(data[group].map(record => record.version));
    expect(host.querySelector('.changelog-summary')?.textContent).toContain(`${label} · ${data[group].length} 个版本`);
    expect(Array.from(host.querySelectorAll('.entry-description'), node => node.textContent))
      .toEqual(data[group].map(record => record.description));
    expect(host.querySelector('.changelog-groups')).toBeNull();
    expect(host.querySelector('.changelog-group')).toBeNull();
    expect(host.textContent).not.toContain('预览版');
    for (const otherLabel of ['正式版', 'Windows Dev', 'Android Dev'].filter(value => value !== label)) {
      expect(host.querySelector('.changelog-summary')?.textContent).not.toContain(otherLabel);
    }
  });

  it('识别完成之前不闪现其他版本的记录', async () => {
    let resolveIdentifier!: (identifier: string) => void;
    environment.getIdentifier.mockReturnValueOnce(new Promise<string>(resolve => { resolveIdentifier = resolve; }));
    await mount('/changelog', dark);
    expect(versions()).toHaveLength(0);
    expect(host.querySelector('[role="status"]')?.textContent).toContain('正在读取应用版本');
    expect(host.querySelector('.changelog-scroll')?.getAttribute('aria-busy')).toBe('true');
    resolveIdentifier('cn.surkaa.pad.dev'); await settle();
    expect(versions()).toEqual(data.windowsDev.map(record => record.version));
    expect(host.querySelector('.changelog-scroll')?.getAttribute('aria-busy')).toBe('false');
  });

  it('识别失败时提示重试，不误显示正式版或其他平台日志', async () => {
    environment.getName.mockRejectedValueOnce(new Error('版本读取失败'));
    await mount('/changelog', dark);
    expect(versions()).toHaveLength(0);
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('版本读取失败');
    environment.getName.mockResolvedValue('SurKaa Pad (Dev)');
    environment.platform.mockReturnValue('android');
    host.querySelector<HTMLButtonElement>('[role="alert"] button')!.click(); await settle();
    expect(versions()).toEqual(data.androidDev.map(record => record.version));
    expect(host.querySelector('[role="alert"]')).toBeNull();
  });

  it('长说明通过感叹号按需打开二级弹窗，不放在关于弹窗中', async () => {
    await mount('/changelog', dark);
    expect(document.querySelector('.details-help-dialog')).toBeNull();
    host.querySelector<HTMLButtonElement>('[aria-label="更新日志说明"]')!.click(); await settle();
    const help = document.querySelector('.details-help-dialog')!;
    expect(help.textContent).toContain('不等同于安装包发布日期');
    expect(help.textContent).toContain('本地时间');
    expect(document.querySelector('.about-dialog')).toBeNull();
    document.querySelector<HTMLButtonElement>('[aria-label="关闭更新日志说明"]')!.click(); await settle();
    expect(document.querySelector('.details-help-dialog')).toBeNull();
    expect(versions()).toHaveLength(data.stable.length);
  });

  it('原来的关于入口仍然正常打开关于弹窗', async () => {
    await mount('/settings', dark);
    host.querySelector<HTMLElement>('.settings-item')!.click(); await settle();
    expect(document.querySelector('.about-dialog')).not.toBeNull();
    expect(router.currentRoute.value.name).toBe('Settings');
    expect(document.querySelector('.about-dialog [aria-label="更新日志"]')).toBeNull();
  });

  it('应用信息接口失败也不影响查看离线更新日志', async () => {
    about.load.mockRejectedValueOnce(new Error('应用信息不可用'));
    await mount('/settings', dark);
    expect(host.textContent).toContain('版本信息读取失败');
    const entering = nextNavigation();
    host.querySelector<HTMLElement>('[aria-label="更新日志"]')!.click(); await entering; await settle();
    expect(router.currentRoute.value.name).toBe('Changelog');
    expect(versions()).toEqual(data.stable.map(record => record.version));
  });
});
