// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, KeepAlive, nextTick, ref, type App} from 'vue';
import {createPinia, disposePinia, setActivePinia, type Pinia} from 'pinia';
import {BottomSheet, Dark, Notify, QLayout, QPageContainer, Quasar} from 'quasar';
import type {MenuItemOptions, MenuOptions} from '@tauri-apps/api/menu';
import DiaryList from '../DiaryList.vue';
import {useDataStore} from '../../../stores/data';
import {CONFIG_LOCAL_CHANGE_EVENT, useConfigStore} from '../../../stores/config';
import {copyTextToClipboard} from '../../../utils/clipboard';
import {collectSyncedSettings} from '../../../utils/syncedSettings';
import type {DiarySummary} from '../../../bindings';

const mocks = vi.hoisted(() => ({platform: 'windows', menu: vi.fn(), push: vi.fn()}));
vi.mock('@tauri-apps/api/menu', () => ({Menu: {new: mocks.menu}}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: () => mocks.platform}));
vi.mock('vue-router', () => ({useRouter: () => ({push: mocks.push}), useRoute: () => ({name: 'DiaryList'})}));
vi.mock('../../../utils/clipboard', () => ({copyTextToClipboard: vi.fn().mockResolvedValue(undefined)}));
vi.mock('../../../utils/api', () => ({default: {
  cmdListDiaryDraftIds: vi.fn().mockResolvedValue([]),
  cmdPageDiaryIds: vi.fn().mockResolvedValue([[], null]),
}}));

let app: App | undefined;
let pinia: Pinia;
let host: HTMLElement;
let active = ref(true);
const ids = ['8215021834823', '8215021834822'];
function summary(id: string): DiarySummary {
  return {id, title: `标题 ${id}`, created: 1_700_000_000_000, updated: 1_700_000_060_000,
    attachmentCount: 0, attachmentCounts: {image: 0, audio: 0, video: 0, file: 0},
    encryptedAttachmentCounts: {image: 0, audio: 0, video: 0, file: 0}, attachmentTotalSize: 0};
}
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(550); await nextTick(); }
async function mount(pinned: string[] = []) {
  pinia = createPinia(); setActivePinia(pinia);
  const config = useConfigStore(); await config.saveNormalConfig('pinned_diary_ids', pinned);
  const data = useDataStore();
  data.diaryIds = [...ids]; data.diarySummaries = Object.fromEntries(ids.map(id => [id, summary(id)]));
  document.body.insertAdjacentHTML('beforeend', '<div id="header-actions"></div><div id="footer-content"></div>');
  host = document.createElement('div'); document.body.append(host); active = ref(true);
  app = createApp({render: () => h(QLayout, {container: true}, () =>
    h(QPageContainer, null, () => h(KeepAlive, null, () => active.value ? h(DiaryList) : h('div', '其他页面'))),
  )});
  app.use(pinia); app.use(Quasar, {plugins: {BottomSheet, Notify}}); app.mount(host); await settle();
  return {config, data};
}
function card(id = ids[0]) { return host.querySelector<HTMLElement>(`[data-diary-id="${id}"]`)!; }
function nativeItems() { return (mocks.menu.mock.lastCall![0] as MenuOptions).items!; }
function nativeItem(label: string) { return nativeItems().find(item => 'text' in item && item.text === label) as MenuItemOptions; }
function sheetItem(label: string) {
  return Array.from(document.querySelectorAll<HTMLElement>('.q-bottom-sheet__item'))
    .find(item => item.textContent?.trim() === label)!;
}
async function open(id = ids[0]) {
  const event = new MouseEvent('contextmenu', {bubbles: true, cancelable: true});
  card(id).querySelector('.preview-title')!.dispatchEvent(event); await settle(); return event;
}
function pointer(type: string, options: PointerEventInit = {}, target: EventTarget = card()) {
  target.dispatchEvent(new PointerEvent(type, {bubbles: true, cancelable: true,
    pointerType: 'touch', isPrimary: true, pointerId: 1, clientX: 20, clientY: 20, ...options}));
}
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks(); localStorage.clear(); sessionStorage.clear(); mocks.platform = 'windows';
  mocks.push.mockResolvedValue(undefined);
  vi.mocked(copyTextToClipboard).mockResolvedValue(undefined);
  mocks.menu.mockImplementation(async () => ({popup: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined)}));
  vi.stubGlobal('IntersectionObserver', class {observe() {} disconnect() {}});
  vi.stubGlobal('ResizeObserver', class {observe() {} disconnect() {}});
});
afterEach(() => {
  app?.unmount(); app = undefined; if (pinia) disposePinia(pinia);
  Dark.set(false); document.body.innerHTML = '';
  vi.unstubAllGlobals(); vi.restoreAllMocks(); vi.useRealTimers();
});

describe.each([false, true])('日记列表上下文菜单（深色=%s）', dark => {
  it('pins/unpins immediately without navigating, and updates the existing cloud-synced setting', async () => {
    Dark.set(dark);
    const {config, data} = await mount();
    const changes = vi.fn(); window.addEventListener(CONFIG_LOCAL_CHANGE_EVENT, changes);
    try {
      expect((await open(ids[1])).defaultPrevented).toBe(true);
      expect(nativeItems().map(item => 'text' in item ? item.text : null)).toEqual(['置顶', '复制日记 ID']);
      expect(mocks.push).not.toHaveBeenCalled(); expect(data.currentId).toBe('');
      nativeItem('置顶').action!(''); await settle();
      expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([ids[1]]);
      expect((await collectSyncedSettings(config)).pinnedDiaryIds).toEqual([ids[1]]);
      expect(changes).toHaveBeenCalledOnce();
      expect(host.querySelector('.pinned-diaries [data-diary-id]')?.getAttribute('data-diary-id')).toBe(ids[1]);
      expect(host.querySelector('.pinned-measurement-clip [data-diary-id]')).toBeNull();
      await open(ids[1]);
      expect(nativeItems().map(item => 'text' in item ? item.text : null)).toEqual(['取消置顶', '复制日记 ID']);
      nativeItem('取消置顶').action!(''); await settle();
      expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([]);
      expect(host.querySelector('.pinned-diaries')).toBeNull();
      expect(mocks.push).not.toHaveBeenCalled();
    } finally { window.removeEventListener(CONFIG_LOCAL_CHANGE_EVENT, changes); }
  });

  it('copies the targeted ID, not the current or first diary ID', async () => {
    Dark.set(dark); const {data} = await mount([ids[1]]); data.currentId = ids[0];
    await open(ids[1]); nativeItem('复制日记 ID').action!(''); await settle();
    expect(copyTextToClipboard).toHaveBeenCalledExactlyOnceWith(ids[1]);
    expect(mocks.push).not.toHaveBeenCalled(); expect(data.currentId).toBe(ids[0]);
  });

  it('long-presses on mobile without opening the diary, using the themed existing sheet', async () => {
    mocks.platform = 'android'; Dark.set(dark); const {config} = await mount();
    pointer('pointerdown', {}, card().querySelector('.preview-title')!);
    await vi.advanceTimersByTimeAsync(500); await settle();
    const sheet = document.querySelector<HTMLElement>('.q-bottom-sheet')!;
    expect(sheet).not.toBeNull(); expect(sheet.classList.contains('q-bottom-sheet--dark')).toBe(dark);
    expect(sheet.style.background).toBe('var(--pad-bg-color-200)');
    expect(sheet.style.color).toBe('var(--pad-text-color-100)');
    expect(Array.from(sheet.querySelectorAll('.q-bottom-sheet__item'), item => item.textContent?.trim()))
      .toEqual(['置顶', '复制日记 ID']);
    await open(); // WebView 的 contextmenu 不应再次创建菜单。
    expect(document.querySelectorAll('.q-bottom-sheet')).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(2500); pointer('pointerup'); card().click(); await settle();
    expect(mocks.push).not.toHaveBeenCalled(); expect(mocks.menu).not.toHaveBeenCalled();
    sheetItem('置顶').click(); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([ids[0]]);
    pointer('pointerdown'); await vi.advanceTimersByTimeAsync(500); await settle();
    expect(sheetItem('取消置顶')).toBeDefined();
    sheetItem('复制日记 ID').click(); await settle();
    expect(copyTextToClipboard).toHaveBeenCalledExactlyOnceWith(ids[0]);
    pointer('pointerdown'); pointer('pointerup'); card().click(); await settle();
    expect(mocks.push).toHaveBeenCalledOnce(); // 下一次正常轻触仍能打开日记。
  });
});

describe('日记菜单边界保护', () => {
  it('preserves concurrent pin updates and never toggles a stale intention or adds duplicates', async () => {
    const {config} = await mount(); await open(); const pin = nativeItem('置顶').action!;
    await config.saveNormalConfig('pinned_diary_ids', [ids[1]]);
    pin(''); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([ids[1], ids[0]]);
    pin(''); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([ids[1], ids[0]]);
    await open(); const unpin = nativeItem('取消置顶').action!;
    await config.saveNormalConfig('pinned_diary_ids', [ids[1]]);
    unpin(''); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([ids[1]]);
  });

  it.each(['scroll', 'move', 'release', 'cancel', 'second-finger'])('cancels mobile long press on %s', async reason => {
    mocks.platform = 'android'; await mount(); pointer('pointerdown');
    if (reason === 'scroll') window.dispatchEvent(new Event('scroll'));
    if (reason === 'move') pointer('pointermove', {clientY: 40}, window);
    if (reason === 'release') pointer('pointerup');
    if (reason === 'cancel') pointer('pointercancel');
    if (reason === 'second-finger') pointer('pointerdown', {pointerId: 2, isPrimary: false});
    await settle();
    expect(document.querySelector('.q-bottom-sheet')).toBeNull(); expect(mocks.push).not.toHaveBeenCalled();
  });

  it('ignores list whitespace, measurement copies and loading cards', async () => {
    const {data} = await mount([ids[0]]);
    host.querySelector('.diary-cards')!.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true}));
    host.querySelector('.pinned-measurement-clip .diary-card')!.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true}));
    data.diarySummaries[ids[1]] = null; await nextTick();
    card(ids[1]).dispatchEvent(new MouseEvent('contextmenu', {bubbles: true})); await settle();
    expect(mocks.menu).not.toHaveBeenCalled();
  });

  it('closes on KeepAlive deactivation and rejects stale actions, then works after returning', async () => {
    const {config} = await mount(); await open(); const pin = nativeItem('置顶').action!;
    const menu = await mocks.menu.mock.results[0].value;
    active.value = false; await settle();
    expect(menu.close).toHaveBeenCalledOnce(); pin(''); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([]);
    active.value = true; await settle(); await open();
    nativeItem('置顶').action!(''); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([ids[0]]);
  });

  it('disables context actions while opening a diary and rejects actions targeting a deleted diary', async () => {
    const {data, config} = await mount(); await open(); const pin = nativeItem('置顶').action!;
    data.deleteSummary(ids[0]); pin(''); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([]);
    let finish!: () => void;
    mocks.push.mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve; }));
    card(ids[1]).click(); await settle();
    await open(ids[1]); expect(mocks.menu).toHaveBeenCalledOnce();
    finish(); await settle();
  });

  it('reports clipboard failures without changing pins or navigating', async () => {
    const {config} = await mount(); const notify = vi.spyOn(app!.config.globalProperties.$q, 'notify');
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(copyTextToClipboard).mockRejectedValueOnce(new Error('clipboard unavailable'));
    await open(); nativeItem('复制日记 ID').action!(''); await settle();
    expect(notify).toHaveBeenCalledWith({type: 'negative', message: '复制日记 ID 失败，请重试'});
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([]); expect(mocks.push).not.toHaveBeenCalled();
  });
});
