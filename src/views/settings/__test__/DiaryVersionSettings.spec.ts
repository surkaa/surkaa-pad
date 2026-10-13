// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import type {DiaryVersionEvent} from '../../../bindings';
import Component from '../DiaryVersionSettings.vue';

const mocks = vi.hoisted(() => ({inspect: vi.fn(), cancel: vi.fn()}));
vi.mock('../../../utils/api', () => ({default: {
  cmdInspectDiaryVersions: mocks.inspect,
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
});
