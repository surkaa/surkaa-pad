import {createPinia, setActivePinia} from 'pinia';
import {ref} from 'vue';
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';

const mocks = vi.hoisted(() => ({
  dialog: vi.fn(),
  hideDialog: vi.fn(),
  dismiss: vi.fn(),
  exit: vi.fn(() => Promise.resolve()),
  notify: vi.fn(),
}));

vi.mock('quasar', () => ({
  useQuasar: () => ({dialog: mocks.dialog, notify: mocks.notify}),
}));
vi.mock('@tauri-apps/plugin-process', () => ({exit: mocks.exit}));
vi.mock('@vueuse/core', () => ({useTimestamp: () => ref(Date.now())}));

import {useTimeoutStore} from '../timeout';

const AUTO_CLOSE_MS = 60 * 60 * 1000;
const WARNING_MS = 60 * 1000;

describe('automatic close warning', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    setActivePinia(createPinia());
    mocks.dialog.mockReset().mockReturnValue({hide: mocks.hideDialog});
    mocks.hideDialog.mockReset();
    mocks.dismiss.mockReset();
    mocks.exit.mockClear();
    mocks.notify.mockReset().mockReturnValue(mocks.dismiss);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('keeps the final warning visible at the top until the app exits', async () => {
    useTimeoutStore().setTimeoutForCloseApp();

    await vi.advanceTimersByTimeAsync(AUTO_CLOSE_MS);
    expect(mocks.dialog).toHaveBeenCalledWith(expect.objectContaining({title: '安全提示'}));
    expect(mocks.notify).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(WARNING_MS / 2);
    expect(mocks.notify).toHaveBeenCalledWith(expect.objectContaining({
      position: 'top',
      timeout: 0,
      group: false,
      message: expect.stringContaining('30 秒后自动关闭'),
    }));

    await vi.advanceTimersByTimeAsync(WARNING_MS / 2);
    expect(mocks.dismiss).toHaveBeenCalledOnce();
    expect(mocks.exit).toHaveBeenCalledWith(0);
  });

  it('clears an old final warning and exit task when the timer is restarted', async () => {
    const store = useTimeoutStore();
    store.setTimeoutForCloseApp();
    await vi.advanceTimersByTimeAsync(AUTO_CLOSE_MS + WARNING_MS / 2);

    store.setTimeoutForCloseApp();
    expect(mocks.dismiss).toHaveBeenCalledOnce();

    await vi.advanceTimersByTimeAsync(WARNING_MS / 2);
    expect(mocks.exit).not.toHaveBeenCalled();
  });

  it('stays paused for hours even if another caller requests a new timer, then restarts for a full hour', async () => {
    const store = useTimeoutStore();
    store.setTimeoutForCloseApp();
    const resume = store.pauseAutoClose();
    expect(store.autoClosePaused).toBe(true);
    expect(store.remainingStr).toBe('已暂停');
    store.setTimeoutForCloseApp();
    await vi.advanceTimersByTimeAsync(3 * AUTO_CLOSE_MS);
    expect(mocks.dialog).not.toHaveBeenCalled();
    expect(mocks.exit).not.toHaveBeenCalled();
    resume();
    expect(store.autoClosePaused).toBe(false);
    await vi.advanceTimersByTimeAsync(AUTO_CLOSE_MS - 1);
    expect(mocks.dialog).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.dialog).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(WARNING_MS);
    expect(mocks.exit).toHaveBeenCalledOnce();
  });

  it.each([0, WARNING_MS / 2])('clears a pending exit and visible warnings when paused during the grace period at +%s ms', async extra => {
    const store = useTimeoutStore();
    store.setTimeoutForCloseApp();
    await vi.advanceTimersByTimeAsync(AUTO_CLOSE_MS + extra);
    const resume = store.pauseAutoClose();
    expect(mocks.hideDialog).toHaveBeenCalledOnce();
    expect(mocks.dismiss).toHaveBeenCalledTimes(extra ? 1 : 0);
    await vi.advanceTimersByTimeAsync(2 * AUTO_CLOSE_MS);
    expect(mocks.notify).toHaveBeenCalledTimes(extra ? 1 : 0);
    expect(mocks.exit).not.toHaveBeenCalled();
    resume();
  });

  it('does not resume before all holders finish and ignores repeated releases', async () => {
    const store = useTimeoutStore();
    const first = store.pauseAutoClose();
    const second = store.pauseAutoClose();
    first(); first();
    expect(store.autoClosePaused).toBe(true);
    await vi.advanceTimersByTimeAsync(2 * AUTO_CLOSE_MS);
    expect(mocks.dialog).not.toHaveBeenCalled();
    second();
    await vi.advanceTimersByTimeAsync(AUTO_CLOSE_MS - 1);
    second(); // 不能因重复释放重新计时或破坏计数。
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.dialog).toHaveBeenCalledOnce();
    expect(store.autoClosePaused).toBe(false);
  });

  it('ignores stale callbacks already queued before the task acquired protection', async () => {
    const scheduled = vi.spyOn(globalThis, 'setTimeout');
    const store = useTimeoutStore(); store.setTimeoutForCloseApp();
    const staleClose = scheduled.mock.calls.find(([, delay]) => delay === AUTO_CLOSE_MS)![0] as () => void;
    const resume = store.pauseAutoClose();
    staleClose(); expect(mocks.dialog).not.toHaveBeenCalled();
    resume(); await vi.advanceTimersByTimeAsync(AUTO_CLOSE_MS);
    const staleExit = scheduled.mock.calls.find(([, delay]) => delay === WARNING_MS)![0] as () => void;
    const staleWarning = scheduled.mock.calls.find(([, delay]) => delay === WARNING_MS / 2)![0] as () => void;
    store.pauseAutoClose();
    staleExit(); staleWarning(); staleClose();
    expect(mocks.exit).not.toHaveBeenCalled();
    expect(mocks.notify).not.toHaveBeenCalled();
    expect(mocks.dialog).toHaveBeenCalledOnce();
  });
});
