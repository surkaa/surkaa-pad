import {createPinia, disposePinia, setActivePinia, type Pinia} from 'pinia';
import {ref} from 'vue';
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';

const mocks = vi.hoisted(() => ({
  dialog: vi.fn(), dismiss: vi.fn(), exit: vi.fn(async () => {}), notify: vi.fn(),
}));
vi.mock('quasar', () => ({useQuasar: () => ({dialog: mocks.dialog, notify: mocks.notify})}));
vi.mock('@tauri-apps/plugin-process', () => ({exit: mocks.exit}));
// 手动推进显示时钟，长时间测试不依赖逐帧 RAF。
const clock = ref(Date.now());
vi.mock('@vueuse/core', () => ({useTimestamp: () => clock}));
import {useTimeoutStore} from '../timeout';

const AUTO_CLOSE_MS = 60 * 60 * 1000;
const WARNING_MS = 60 * 1000;
let pinia: Pinia;
async function advance(ms: number) {
  await vi.advanceTimersByTimeAsync(ms);
  clock.value = Date.now();
}
function callbackFor(scheduled: {mock: {calls: unknown[][]}}, delay: number) {
  return scheduled.mock.calls.find(args => args[1] === delay)![0] as () => void;
}

describe('automatic close countdown', () => {
  beforeEach(() => {
    vi.useFakeTimers(); vi.setSystemTime(new Date('2026-10-28T08:00:00Z'));
    clock.value = Date.now(); pinia = createPinia(); setActivePinia(pinia);
    vi.clearAllMocks(); mocks.notify.mockReturnValue(mocks.dismiss);
  });
  afterEach(() => {
    useTimeoutStore(pinia).clearCloseTimers(); disposePinia(pinia);
    vi.restoreAllMocks(); vi.clearAllTimers(); vi.useRealTimers();
  });

  it('is inactive until started and enters red only at the last 10% (6 minutes)', async () => {
    const store = useTimeoutStore();
    expect(store.remainingStr).toBe('00:00'); expect(store.isExpiring).toBe(false);
    store.setTimeoutForCloseApp();
    expect(store.remainingStr).toBe('60:00'); expect(store.isExpiring).toBe(false);
    await advance(AUTO_CLOSE_MS * 0.9 - 1);
    expect(store.remainingStr).toBe('06:01'); expect(store.isExpiring).toBe(false);
    await advance(1);
    expect(store.remainingStr).toBe('06:00'); expect(store.isExpiring).toBe(true);
    await advance(AUTO_CLOSE_MS * 0.1 - 1);
    expect(store.remainingStr).toBe('00:01'); expect(store.isExpiring).toBe(true);
    expect(mocks.notify).not.toHaveBeenCalled(); expect(mocks.exit).not.toHaveBeenCalled();
  });

  it('restarts at red 01:00 after an hour, warns once at the top and closes after that minute', async () => {
    const store = useTimeoutStore(); store.setTimeoutForCloseApp();
    await advance(AUTO_CLOSE_MS);
    expect(store.remainingStr).toBe('01:00'); expect(store.isExpiring).toBe(true);
    expect(mocks.dialog).not.toHaveBeenCalled();
    expect(mocks.notify).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
      position: 'top', timeout: 0, group: false, classes: 'pad-auto-close-warning',
      message: expect.stringContaining('1 分钟后自动关闭'),
    }));
    await advance(WARNING_MS / 2);
    expect(store.remainingStr).toBe('00:30'); expect(store.isExpiring).toBe(true);
    expect(mocks.notify).toHaveBeenCalledTimes(1); expect(mocks.dismiss).not.toHaveBeenCalled();
    await advance(WARNING_MS / 2 - 1);
    expect(store.remainingStr).toBe('00:01'); expect(mocks.exit).not.toHaveBeenCalled();
    await advance(1);
    expect(store.remainingStr).toBe('00:00'); expect(store.isExpiring).toBe(true);
    expect(mocks.dismiss).toHaveBeenCalledOnce(); expect(mocks.exit).toHaveBeenCalledExactlyOnceWith(0);
    await advance(AUTO_CLOSE_MS);
    expect(mocks.notify).toHaveBeenCalledTimes(1); expect(mocks.exit).toHaveBeenCalledTimes(1);
  });

  it('synchronizes a new deadline immediately even if the display clock has not ticked', async () => {
    const store = useTimeoutStore(); store.setTimeoutForCloseApp();
    await vi.advanceTimersByTimeAsync(AUTO_CLOSE_MS); // 不手动更新 clock。
    expect(store.remainingStr).toBe('01:00');
    await advance(WARNING_MS / 2);
    store.setTimeoutForCloseApp();
    expect(store.remainingStr).toBe('60:00'); expect(store.isExpiring).toBe(false);
    expect(mocks.dismiss).toHaveBeenCalledOnce();
    await advance(WARNING_MS / 2); expect(mocks.exit).not.toHaveBeenCalled();
  });

  it('does not let a previous ordinary timer warn after a restart', async () => {
    const store = useTimeoutStore(); store.setTimeoutForCloseApp();
    await advance(AUTO_CLOSE_MS * 0.95); expect(store.isExpiring).toBe(true);
    store.setTimeoutForCloseApp(); expect(store.isExpiring).toBe(false);
    await advance(AUTO_CLOSE_MS * 0.05);
    expect(mocks.notify).not.toHaveBeenCalled(); expect(mocks.exit).not.toHaveBeenCalled();
  });

  it('clearing cancels the visible warning and pending exit without leaving a red timer', async () => {
    const store = useTimeoutStore(); store.setTimeoutForCloseApp();
    await advance(AUTO_CLOSE_MS); store.clearCloseTimers();
    expect(store.remainingStr).toBe('00:00'); expect(store.isExpiring).toBe(false);
    expect(mocks.dismiss).toHaveBeenCalledOnce();
    await advance(2 * AUTO_CLOSE_MS); expect(mocks.exit).not.toHaveBeenCalled();
  });

  it('stays paused for hours despite restart requests and resumes for a full hour', async () => {
    const store = useTimeoutStore(); store.setTimeoutForCloseApp();
    await advance(AUTO_CLOSE_MS * 0.95);
    const resume = store.pauseAutoClose();
    expect(store.autoClosePaused).toBe(true); expect(store.remainingStr).toBe('已暂停');
    expect(store.isExpiring).toBe(false);
    store.setTimeoutForCloseApp(); await advance(3 * AUTO_CLOSE_MS);
    expect(mocks.notify).not.toHaveBeenCalled(); expect(mocks.exit).not.toHaveBeenCalled();
    resume(); expect(store.autoClosePaused).toBe(false);
    expect(store.remainingStr).toBe('60:00'); expect(store.isExpiring).toBe(false);
    await advance(AUTO_CLOSE_MS - 1); expect(mocks.notify).not.toHaveBeenCalled();
    await advance(1); expect(mocks.notify).toHaveBeenCalledOnce();
    await advance(WARNING_MS); expect(mocks.exit).toHaveBeenCalledOnce();
  });

  it.each([0, WARNING_MS / 2])('pausing during the final minute clears warning/exit at +%s ms', async extra => {
    const store = useTimeoutStore(); store.setTimeoutForCloseApp();
    await advance(AUTO_CLOSE_MS + extra);
    const resume = store.pauseAutoClose();
    expect(mocks.dismiss).toHaveBeenCalledOnce(); expect(store.isExpiring).toBe(false);
    await advance(2 * AUTO_CLOSE_MS);
    expect(mocks.notify).toHaveBeenCalledTimes(1); expect(mocks.exit).not.toHaveBeenCalled();
    resume(); expect(store.remainingStr).toBe('60:00');
  });

  it('does not resume before all holders finish and ignores repeated releases', async () => {
    const store = useTimeoutStore();
    const first = store.pauseAutoClose(), second = store.pauseAutoClose();
    first(); first(); expect(store.autoClosePaused).toBe(true);
    await advance(2 * AUTO_CLOSE_MS); expect(mocks.notify).not.toHaveBeenCalled();
    second(); await advance(AUTO_CLOSE_MS - 1);
    second(); await advance(1);
    expect(mocks.notify).toHaveBeenCalledOnce(); expect(store.autoClosePaused).toBe(false);
  });

  it('rejects stale queued callbacks after pause/restart and repeated callbacks in the same generation', async () => {
    const scheduled = vi.spyOn(globalThis, 'setTimeout');
    const store = useTimeoutStore(); store.setTimeoutForCloseApp();
    const staleClose = callbackFor(scheduled, AUTO_CLOSE_MS);
    const resume = store.pauseAutoClose();
    staleClose(); expect(mocks.notify).not.toHaveBeenCalled();
    resume(); await advance(AUTO_CLOSE_MS);
    const close = scheduled.mock.calls.filter(([, delay]) => delay === AUTO_CLOSE_MS).slice(-1)[0][0] as () => void;
    close(); expect(mocks.notify).toHaveBeenCalledTimes(1);
    const staleExit = callbackFor(scheduled, WARNING_MS);
    store.pauseAutoClose(); staleExit(); staleClose(); close();
    expect(mocks.exit).not.toHaveBeenCalled(); expect(mocks.notify).toHaveBeenCalledTimes(1);
  });
});
