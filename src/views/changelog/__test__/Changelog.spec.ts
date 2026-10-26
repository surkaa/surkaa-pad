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
function groupButton(label: string) { return host.querySelector<HTMLButtonElement>(`.changelog-group[aria-label="${label}"]`)!; }
function versions() { return Array.from(host.querySelectorAll('.changelog-entry'), node => node.getAttribute('data-version')); }
function nextNavigation() {
  return new Promise<void>(resolve => {
    const stop = router.afterEach(() => { stop(); resolve(); });
  });
}
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks();
  about.load.mockResolvedValue({
    appName: 'SurKaa Pad', appVersion: '0.8.16', gitCommit: '12345678', identifier: 'cn.surkaa.pad',
    tauriVersion: '2', platform: 'Windows', architecture: 'x86_64', osVersion: '11',
  });
});
afterEach(() => {
  app?.unmount(); app = undefined; router?.options.history.destroy(); if (pinia) disposePinia(pinia);
  document.body.innerHTML = ''; Dark.set(false); vi.useRealTimers(); vi.restoreAllMocks();
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

  it('默认显示全部正式版，包括0.4.10之前的版本，按JSON顺序展示纯文本及本地日期', async () => {
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
    expect(groupButton('正式版').getAttribute('aria-pressed')).toBe('true');
    expect(about.load).not.toHaveBeenCalled(); // 查看本地日志不依赖 Tauri 信息接口。
    expect(document.body.classList.contains('body--dark')).toBe(dark);
  });

  it('四个版本序列独立切换，不把正式版与开发版的同名版本混在一起', async () => {
    await mount('/changelog', dark);
    for (const [group, label] of [['preview', '预览版'], ['windowsDev', 'Windows Dev'], ['androidDev', 'Android Dev'], ['stable', '正式版']] as const) {
      groupButton(label).click(); await settle();
      expect(versions()).toEqual(data[group].map(record => record.version));
      expect(groupButton(label).getAttribute('aria-pressed')).toBe('true');
      expect(host.querySelector('.changelog-summary')?.textContent).toContain(`${label} · ${data[group].length} 个版本`);
      expect(Array.from(host.querySelectorAll('.entry-description'), node => node.textContent))
        .toEqual(data[group].map(record => record.description));
    }
  });

  it('切换分组后从顶部查看；重复点击当前分组不打断阅读位置', async () => {
    await mount('/changelog', dark);
    const scroll = host.querySelector<HTMLElement>('.changelog-scroll')!;
    scroll.scrollTop = 480;
    groupButton('正式版').click(); await settle();
    expect(scroll.scrollTop).toBe(480);
    groupButton('Windows Dev').click(); await settle();
    expect(scroll.scrollTop).toBe(0);
    expect(host.querySelector('[data-version="0.8.16"]')).toBeNull();
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
