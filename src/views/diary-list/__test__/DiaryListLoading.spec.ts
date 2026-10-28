// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, KeepAlive, nextTick, ref, type App} from 'vue';
import {createPinia, disposePinia, setActivePinia, type Pinia} from 'pinia';
import {Dark, Notify, QLayout, QPageContainer, Quasar} from 'quasar';
import DiaryList from '../DiaryList.vue';
import {useDataStore} from '../../../stores/data';
import {useConfigStore} from '../../../stores/config';
import api from '../../../utils/api';
import {useTimeoutStore} from '../../../stores/timeout';

const environment = vi.hoisted(() => ({platform: 'windows'}));
const clock = ref(Date.now());
vi.mock('@vueuse/core', async importOriginal => ({
  ...await importOriginal<typeof import('@vueuse/core')>(), useTimestamp: () => clock,
}));
vi.mock('@tauri-apps/plugin-process', () => ({exit: vi.fn()}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: () => environment.platform}));
vi.mock('vue-router', () => ({useRouter: () => ({push: vi.fn()}), useRoute: () => ({name: 'DiaryList'})}));
vi.mock('../../../composables/useDiaryListContextMenu', () => ({useDiaryListContextMenu: () => {}}));
vi.mock('../../../composables/useOpenDiaryDetail', () => ({useOpenDiaryDetail: () => ({openDiary: vi.fn()})}));
vi.mock('../../../utils/api', () => ({default: {
  cmdListDiaryDraftIds: vi.fn().mockResolvedValue([]), cmdPageDiaryIds: vi.fn(),
}}));

let app: App | undefined;
let pinia: Pinia;
let host: HTMLElement;
let active = ref(true);
const pinned = Array.from({length: 8}, (_, i) => String(8215021834800 + i));
const ordinary = Array.from({length: 5}, (_, i) => String(8215021834900 + i));
const sizes = {viewport: 600, naturalReady: false};
const observers = new Map<Element, Set<() => void>>();
function notifyResize(element: Element) { for (const notify of [...(observers.get(element) ?? [])]) notify(); }
function cardsHeight(element: Element) {
  return [...element.querySelectorAll<HTMLElement>('[data-diary-id]')]
    .reduce((sum, card) => sum + (card.classList.contains('diary-card--pinned') ? 40 : 180), 0);
}
function deferredPage() {
  let resolve!: (page: [string[], string | null]) => void;
  const promise = new Promise<[string[], string | null]>(finish => { resolve = finish; });
  return {promise, resolve};
}
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(500); await nextTick(); }
async function mount(pinnedIds = pinned, dark = false) {
  pinia = createPinia(); setActivePinia(pinia);
  const config = useConfigStore(pinia); await config.saveNormalConfig('pinned_diary_ids', pinnedIds);
  const data = useDataStore(pinia);
  document.body.insertAdjacentHTML('beforeend', '<div id="header-actions"></div><div id="footer-content"></div>');
  host = document.createElement('div'); document.body.append(host); active = ref(true);
  app = createApp({render: () => h(QLayout, {container: true}, () =>
    h(QPageContainer, null, () => h(KeepAlive, null, () => active.value ? h(DiaryList) : h('div', '其他页面'))),
  )});
  app.use(pinia); app.use(Quasar, {plugins: {Notify}}); Dark.set(dark);
  app.mount(host); await settle();
  return data;
}
async function compact() {
  sizes.naturalReady = true;
  notifyResize(host.querySelector('.pinned-natural-layout')!);
  await nextTick();
  notifyResize(host.querySelector('.diary-cards')!);
  await settle();
}
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks(); localStorage.clear(); sessionStorage.clear();
  clock.value = Date.now();
  environment.platform = 'windows'; Object.assign(sizes, {viewport: 600, naturalReady: false}); observers.clear();
  vi.mocked(api.cmdPageDiaryIds).mockReset().mockResolvedValue([ordinary, null]);
  vi.stubGlobal('IntersectionObserver', class {observe() {} disconnect() {}});
  vi.stubGlobal('ResizeObserver', class {
    private targets = new Map<Element, () => void>();
    constructor(private callback: ResizeObserverCallback) {}
    observe(target: Element) {
      const notify = () => this.callback([], this as unknown as ResizeObserver);
      this.targets.set(target, notify);
      if (!observers.has(target)) observers.set(target, new Set());
      observers.get(target)!.add(notify);
    }
    unobserve(target: Element) { observers.get(target)?.delete(this.targets.get(target)!); this.targets.delete(target); }
    disconnect() { for (const target of this.targets.keys()) this.unobserve(target); }
  });
  // happy-dom 无布局引擎，模拟 Windows 首帧置顶很高、下一帧测量后紧缩的顺序。
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return this.id === 'list' && this.isConnected ? sizes.viewport : 0;
  });
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return this.id === 'list' ? Math.max(sizes.viewport, cardsHeight(this) + 124) : 0;
  });
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    const height = this.id === 'list' ? this.clientHeight
      : this.classList.contains('pinned-natural-layout') ? (sizes.naturalReady ? this.children.length * 180 : 0)
      : this.classList.contains('diary-cards') ? cardsHeight(this) : 0;
    return new DOMRect(0, 0, environment.platform === 'windows' ? 1024 : 390, height);
  });
});
afterEach(() => {
  if (pinia) useTimeoutStore(pinia).clearCloseTimers();
  app?.unmount(); app = undefined; if (pinia) disposePinia(pinia);
  Dark.set(false); document.body.innerHTML = '';
  vi.clearAllTimers(); vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks();
});

describe.each([['windows', false], ['windows', true], ['android', false], ['android', true]] as const)
('普通日记首次加载（%s，深色=%s）', (currentPlatform, dark) => {
  beforeEach(() => { environment.platform = currentPlatform; });
  it('底部计时只在最后 10% 变红，到时重新显示红色一分钟，暂停恢复清除红色', async () => {
    await mount([], dark);
    const scheduled = vi.spyOn(globalThis, 'setTimeout');
    const timeout = useTimeoutStore(pinia); timeout.setTimeoutForCloseApp(); await nextTick();
    const started = Date.now();
    const footer = () => document.querySelector<HTMLElement>('#footer-content .auto-close-countdown')!;
    expect(footer().textContent).toContain('60:00');
    expect(footer().classList.contains('auto-close-countdown--urgent')).toBe(false);
    clock.value = started + 54 * 60_000 - 1; await nextTick();
    expect(footer().classList.contains('auto-close-countdown--urgent')).toBe(false);
    clock.value += 1; await nextTick();
    expect(footer().textContent).toContain('06:00');
    expect(footer().classList.contains('auto-close-countdown--urgent')).toBe(true);
    // 直接触发到期回调，避免测试列表内其他 RAF 长时间运行。
    vi.setSystemTime(started + 60 * 60_000);
    const beginGrace = scheduled.mock.calls.find(([, delay]) => delay === 60 * 60_000)![0] as () => void;
    beginGrace(); await nextTick();
    expect(footer().textContent).toContain('01:00');
    expect(footer().classList.contains('auto-close-countdown--urgent')).toBe(true);
    expect(document.querySelector('.q-dialog')).toBeNull();
    const resume = timeout.pauseAutoClose(); await nextTick();
    expect(footer().textContent).toContain('已暂停');
    expect(footer().classList.contains('auto-close-countdown--urgent')).toBe(false);
    resume(); await nextTick();
    expect(footer().textContent).toContain('60:00');
    expect(footer().classList.contains('auto-close-countdown--urgent')).toBe(false);
  });

  it('高置顶区尚未紧缩、也没有用户滚动时，仍获取首批普通日记', async () => {
    const data = await mount(pinned, dark);
    expect(api.cmdPageDiaryIds).toHaveBeenCalledExactlyOnceWith(null);
    expect(data.diaryIds).toEqual([...pinned, ...ordinary]);
    await compact();
    expect(host.querySelectorAll('.pinned-diaries .diary-card--pinned')).toHaveLength(pinned.length);
    expect(host.querySelectorAll('.diary-cards > [data-diary-id]')).toHaveLength(ordinary.length);
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(1);
  });

  it('第一页全是置顶时，紧缩腾出空间后自动继续分页，不需要滚动才显示普通日记', async () => {
    const first = deferredPage();
    vi.mocked(api.cmdPageDiaryIds).mockReturnValueOnce(first.promise).mockResolvedValueOnce([ordinary, 'next-page']);
    const data = await mount(pinned, dark);
    const scroll = host.querySelector<HTMLElement>('#list')!;
    // 先保证首次触底检查已进行；响应到达前回到顶部，普通尺寸的置顶仍覆盖整屏。
    scroll.scrollTop = 2000; scroll.dispatchEvent(new Event('scroll')); await settle(); scroll.scrollTop = 0;
    first.resolve([pinned, 'pinned-page']); await settle();
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(1);
    await compact();
    expect(vi.mocked(api.cmdPageDiaryIds).mock.calls.map(([cursor]) => cursor)).toEqual([null, 'pinned-page']);
    expect(data.diaryIds).toEqual([...pinned, ...ordinary]);
    await settle(); expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(2); // 满屏后不预取所有后续页。
  });

  it.each([{pinnedIds: []}, {pinnedIds: pinned.slice(0, 1)}])('没有置顶或不需要紧缩时保持正常首次加载：$pinnedIds', async ({pinnedIds}) => {
    const data = await mount(pinnedIds, dark);
    expect(api.cmdPageDiaryIds).toHaveBeenCalledExactlyOnceWith(null);
    expect(data.diaryIds).toEqual([...pinnedIds, ...ordinary]);
  });
});

describe('布局变化后的分页保护', () => {
  it('请求失败后尺寸变化或重新激活不会反复自动重试', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(api.cmdPageDiaryIds).mockRejectedValueOnce(new Error('分页读取失败'));
    await mount(); await compact();
    sizes.viewport = 3000; notifyResize(host.querySelector('#list')!); await settle();
    active.value = false; await settle(); active.value = true; await settle();
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(1);
  });

  it('反复尺寸通知不会并发请求，读到末页后也不会重新加载', async () => {
    const first = deferredPage(); vi.mocked(api.cmdPageDiaryIds).mockReturnValueOnce(first.promise);
    await mount(); await compact();
    for (let i = 0; i < 5; i++) { notifyResize(host.querySelector('.diary-cards')!); await settle(); }
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(1);
    first.resolve([ordinary, null]); await settle();
    sizes.viewport = 3000;
    for (let i = 0; i < 5; i++) { notifyResize(host.querySelector('#list')!); await settle(); }
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(1);
  });

  it('紧缩状态不变但窗口变高时重新检查是否需要加载', async () => {
    vi.mocked(api.cmdPageDiaryIds).mockResolvedValueOnce([ordinary, 'next-page']).mockResolvedValueOnce([[], null]);
    await mount(); await compact();
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(1);
    sizes.viewport = 1500; notifyResize(host.querySelector('#list')!); await settle();
    expect(api.cmdPageDiaryIds).toHaveBeenLastCalledWith('next-page');
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(2);
  });

  it('缓存隐藏期间不继续分页，返回后再按当前布局继续', async () => {
    const first = deferredPage();
    vi.mocked(api.cmdPageDiaryIds).mockReturnValueOnce(first.promise).mockResolvedValueOnce([ordinary, null]);
    await mount(); await compact();
    const cards = host.querySelector('.diary-cards')!;
    active.value = false; await settle();
    first.resolve([pinned, 'after-hidden']); await settle();
    notifyResize(cards); await settle();
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(1);
    active.value = true; await settle();
    expect(api.cmdPageDiaryIds).toHaveBeenLastCalledWith('after-hidden');
    expect(api.cmdPageDiaryIds).toHaveBeenCalledTimes(2);
  });
});
