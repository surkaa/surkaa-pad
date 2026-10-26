// @vitest-environment happy-dom
import {afterAll, afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, type App} from 'vue';
import {createPinia, disposePinia, type Pinia} from 'pinia';
import {RouterView} from 'vue-router';
import {Dark, Quasar} from 'quasar';
import router from '../../../router';
import {keepAliveIncludes, removeCache} from '../../../composables/useKeepAlive';

// 使用真实设置页、更新日志、Layout、路由守卫及 KeepAlive，仅隔离原生接口和无关设置模块。
vi.mock('../../unlock/Unlock.vue', () => ({default: {render: () => null}}));
vi.mock('../../diary-list/DiaryList.vue', () => ({default: {name: 'DiaryList', render: () => null}}));
vi.mock('../../ai-assistant/AiAssistant.vue', () => ({default: {name: 'AiAssistant', render: () => null}}));
vi.mock('../AiSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../AttachmentSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../ToolbarSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../AttachmentCacheSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../DiaryVersionSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../VaultBootstrapSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../DiagnosticLogSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../EditorShortcutSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../LocalStorageSettings.vue', () => ({default: {render: () => null}}));
vi.mock('../PinnedDiarySettings.vue', () => ({default: {render: () => null}}));
vi.mock('../../../components/DisableRemoteStorageDialog.vue', () => ({default: {render: () => null}}));
vi.mock('../../../stores/config', async () => {
  const {ref} = await import('vue');
  return {useConfigStore: () => ({
    useTauriConfig: (key: string) => ref(key === 'app-theme' ? 'system' : key === 'biometric_password_interval_days' ? 7 : false),
  })};
});
vi.mock('../../../composables/useRemoteStorageSettings', async () => {
  const {ref} = await import('vue');
  return {useRemoteStorageSettings: () => ({
    remoteEnabled: ref(false), remoteStorageBusy: ref(false), localStorageRefreshRevision: ref(0),
    showOssConfigDialog: ref(false), showDisableRemotePlan: ref(false), planningDisableRemote: ref(false),
    showSyncProgress: ref(false), syncProgress: ref(0), syncTotal: ref(0),
    syncStatusText: ref(''), syncCurrentFile: ref(''), syncFileDetail: ref(''),
    ossConfig: ref({akid: '', aks: '', bucket: '', endpoint: ''}),
  })};
});
const native = vi.hoisted(() => ({platform: vi.fn(), about: vi.fn()}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: native.platform}));
vi.mock('../../../utils/aboutInfo', () => ({
  loadAboutInfo: native.about, formatAboutSummary: () => '版本 0.8.16',
}));

let app: App | undefined;
let pinia: Pinia;
let host: HTMLElement;
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick(); }
async function mount(currentPlatform: string, dark: boolean) {
  native.platform.mockReturnValue(currentPlatform);
  pinia = createPinia();
  app = createApp({render: () => h(RouterView)});
  app.use(pinia); app.use(Quasar); app.use(router);
  await router.push('/diary-list'); await router.isReady();
  host = document.createElement('div'); document.body.append(host);
  Dark.set(dark); app.mount(host); await settle();
  await router.push('/settings'); await settle();
}
function scroll() { return host.querySelector<HTMLElement>('.settings-content')!; }
function nextNavigation() {
  return new Promise<void>(resolve => {
    const stop = router.afterEach(() => { stop(); resolve(); });
  });
}
async function openChangelog() {
  const entering = nextNavigation();
  host.querySelector<HTMLElement>('[aria-label="更新日志"]')!.click(); await entering; await settle();
}
async function back() { const returning = nextNavigation(); router.back(); await returning; await settle(); }

beforeEach(() => {
  vi.useFakeTimers(); vi.stubEnv('DEV', true); removeCache('Settings'); removeCache('AiAssistant');
  native.about.mockReset().mockResolvedValue({appName: 'SurKaa Pad'});
});
afterEach(async () => {
  // 在 Pinia 销毁之前完成路由清理，确保 afterEach 的布局状态重置仍有有效 store。
  await router.replace('/'); await settle();
  app?.unmount(); app = undefined; disposePinia(pinia);
  document.body.innerHTML = ''; Dark.set(false);
  vi.unstubAllEnvs(); vi.useRealTimers(); vi.restoreAllMocks();
});
afterAll(() => router.options.history.destroy());

describe.each([['windows', false], ['windows', true], ['android', false], ['android', true]] as const)
('设置页返回位置：%s 深色=%s', (currentPlatform, dark) => {
  it('从更新日志后退时复用设置页，在重新布局后恢复自己的滚动位置', async () => {
    await mount(currentPlatform, dark);
    const original = scroll(); original.scrollTop = 2345;
    expect(router.currentRoute.value.meta.keepAlive).toBe(true);
    await openChangelog();
    expect(router.currentRoute.value.name).toBe('Changelog');
    expect(scroll()).toBeNull();
    expect(keepAliveIncludes.value).toContain('Settings');
    // 模拟浏览器隐藏 KeepAlive 节点时清零；返回不能依赖被隐藏 DOM 的当前位置。
    original.scrollTop = 0;
    host.querySelector<HTMLElement>('.changelog-scroll')!.scrollTop = 555;
    await back();
    expect(router.currentRoute.value.name).toBe('Settings');
    expect(scroll()).toBe(original);
    expect(scroll().scrollTop).toBe(2345);
    expect(native.about).toHaveBeenCalledTimes(1);
    expect(document.body.classList.contains('body--dark')).toBe(dark);
  });

  it('多次往返使用最新位置，也允许用户主动滚回顶部', async () => {
    await mount(currentPlatform, dark);
    for (const position of [1600, 920, 0]) {
      scroll().scrollTop = position;
      await openChangelog(); await back();
      expect(scroll().scrollTop).toBe(position);
    }
    expect(native.about).toHaveBeenCalledTimes(1);
  });

  it.each(['DiaryList', 'AiAssistant', 'Unlock'])('离开设置/更新日志流程到 %s 后销毁缓存，下次正常新建', async target => {
    await mount(currentPlatform, dark);
    const original = scroll(); original.scrollTop = 1800;
    await openChangelog();
    await router.push({name: target}); await settle();
    expect(keepAliveIncludes.value).not.toContain('Settings');
    await router.push('/settings'); await settle();
    expect(scroll()).not.toBe(original);
    expect(scroll().scrollTop).toBe(0);
    expect(native.about).toHaveBeenCalledTimes(2);
  });
});
