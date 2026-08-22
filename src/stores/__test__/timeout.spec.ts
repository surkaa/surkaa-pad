import {createPinia, setActivePinia} from 'pinia';
import {ref} from 'vue';
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';

const mocks = vi.hoisted(() => ({
  dialog: vi.fn(),
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
    mocks.dialog.mockReset();
    mocks.dismiss.mockReset();
    mocks.exit.mockClear();
    mocks.notify.mockReset().mockReturnValue(mocks.dismiss);
  });

  afterEach(() => {
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
});
