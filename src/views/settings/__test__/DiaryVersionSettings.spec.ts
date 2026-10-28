// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import type {DiaryVersionEvent} from '../../../bindings';
import Component from '../DiaryVersionSettings.vue';

const mocks = vi.hoisted(() => ({inspect: vi.fn(), upgrade: vi.fn(), cancel: vi.fn(), invalidate: vi.fn()}));
vi.mock('../../../stores/data', () => ({useDataStore: () => ({invalidateDiaryList: mocks.invalidate})}));
vi.mock('../../../utils/api', () => ({default: {
  cmdInspectDiaryVersions: mocks.inspect,
  cmdUpgradeDiaryVersions: mocks.upgrade,
  cmdCancelTask: mocks.cancel,
}}));
vi.mock('@tauri-apps/api/core', () => ({Channel: class {
  onmessage: ((event: DiaryVersionEvent) => void) | undefined;
}}));

let app: App | undefined;
afterEach(() => {
  app?.unmount();
  app = undefined;
  Dark.set(false);
  document.body.innerHTML = '';
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe('diary version report actions', () => {
  it.each([false, true])('highlights cancel at the right after inspection (dark=%s)', async dark => {
    vi.useFakeTimers();
    mocks.inspect.mockImplementation(async channel => {
      channel.onmessage({event: 'completed', data: {report: {
        scope: 'local', currentVersion: 7, totalDiaries: 2, currentDiaries: 2,
        legacyDiaries: 0, newerDiaries: 0, failedDiaries: 0, failedDiaryIds: [],
      }}});
      return 'inspection-token';
    });
    const host = document.createElement('div');
    document.body.append(host);
    app = createApp(Component, {remoteEnabled: false});
    app.use(Quasar);
    app.mount(host);
    Dark.set(dark);
    host.querySelector<HTMLElement>('.settings-item')!.click();
    await vi.advanceTimersByTimeAsync(500);
    await nextTick();
    const buttons = () => Array.from(document.querySelectorAll<HTMLButtonElement>('.q-card__actions .q-btn'));
    expect(buttons().map(button => button.textContent?.trim())).toEqual(['关闭', '开始检查']);
    buttons()[1].click();
    await vi.advanceTimersByTimeAsync(500);
    await nextTick();

    expect(buttons().map(button => button.textContent?.trim())).toEqual(['重新检查', '取消']);
    expect(buttons()[0].classList.contains('bg-primary')).toBe(false);
    expect(buttons()[1].classList.contains('bg-primary')).toBe(true);
    expect(document.querySelector('.q-card__actions')?.classList.contains('justify-end')).toBe(true);
    buttons()[1].click();
    await vi.advanceTimersByTimeAsync(500);
    expect(document.querySelector('.version-dialog')).toBeNull();
    expect(mocks.cancel).not.toHaveBeenCalled();
  });

  it.each([false, true])('confirms a unified upgrade, removes generic legacy warning and shows partial results (dark=%s)', async dark => {
    vi.useFakeTimers();
    const legacy = {scope: 'cloud' as const, currentVersion: 8, totalDiaries: 3, processedDiaries: 3,
      currentDiaries: 1, legacyDiaries: 2, newerDiaries: 0, failedDiaries: 0, failedDiaryIds: [], upgrade: null};
    mocks.inspect.mockImplementation(async channel => {
      channel.onmessage({event: 'completed', data: {report: legacy}}); return 'read-token';
    });
    let upgradeChannel: {onmessage: (event: DiaryVersionEvent) => void} | undefined;
    mocks.upgrade.mockImplementation(async channel => { upgradeChannel = channel; return 'upgrade-token'; });
    mocks.cancel.mockResolvedValue(true);
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(Component, {remoteEnabled: true}); app.use(Quasar); app.mount(host); Dark.set(dark);
    host.querySelector<HTMLElement>('.settings-item')!.click();
    const settle = async () => { await vi.advanceTimersByTimeAsync(500); await nextTick(); };
    const click = async (label: string) => {
      const button = [...document.querySelectorAll<HTMLButtonElement>('.q-card__actions button')]
        .find(button => button.textContent?.trim() === label)!;
      expect(button).toBeTruthy(); button.click(); await settle();
    };
    await settle(); await click('开始检查');
    expect(document.body.textContent).not.toContain('不受支持的旧版日记');
    await click('统一升级');
    expect(mocks.upgrade).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('旧版应用可能无法再读取');
    await click('返回'); expect(mocks.upgrade).not.toHaveBeenCalled();
    await click('统一升级'); await click('确认升级');
    expect(mocks.upgrade).toHaveBeenCalledTimes(1);
    expect(document.body.textContent).toContain('正在统一升级日记');
    upgradeChannel!.onmessage({event: 'progress', data: {processed: 1, total: 3, diaryId: '123', outcome: 'upgraded'}});
    await settle(); expect(mocks.invalidate).toHaveBeenCalledTimes(1);
    await click('取消'); expect(mocks.cancel).toHaveBeenCalledWith('upgrade-token');
    upgradeChannel!.onmessage({event: 'cancelled', data: {report: {...legacy, processedDiaries: 1,
      currentDiaries: 1, legacyDiaries: 0, upgrade: {upgraded: 1, skipped: 0, issues: []}}}});
    await settle();
    expect(document.body.textContent).toContain('已处理 1 / 3 篇');
    expect(document.body.textContent).not.toContain('上一次完整检查');
    expect(document.body.textContent).toContain('已升级 1 篇');
  });

  it('cancels a task whose token arrives only after the settings component unmounts', async () => {
    vi.useFakeTimers();
    let resolve!: (token: string) => void;
    mocks.inspect.mockReturnValue(new Promise<string>(r => { resolve = r; }));
    mocks.cancel.mockResolvedValue(true);
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(Component, {remoteEnabled: false}); app.use(Quasar); app.mount(host);
    host.querySelector<HTMLElement>('.settings-item')!.click(); await vi.advanceTimersByTimeAsync(500);
    [...document.querySelectorAll<HTMLButtonElement>('.q-card__actions button')]
      .find(button => button.textContent?.trim() === '开始检查')!.click();
    await nextTick(); app.unmount(); app = undefined;
    resolve('late-token'); await vi.advanceTimersByTimeAsync(0);
    expect(mocks.cancel).toHaveBeenCalledWith('late-token');
  });
});
