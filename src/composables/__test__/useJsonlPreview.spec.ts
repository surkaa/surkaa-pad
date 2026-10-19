// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App} from 'vue';
import {useJsonlPreview} from '../useJsonlPreview';
import type {JsonlPage} from '../../utils/jsonlPreview';

let app: App | undefined;
function page(start = 1, count = 10, nextCursor: string | null = 'next'): JsonlPage {
  return {lines: Array.from({length: count}, (_, i) => ({lineNumber: start + i, text: JSON.stringify({i: start + i}), error: null})), nextCursor};
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes; }); return {promise, resolve}; }
async function settle() { for (let i = 0; i < 5; i++) await nextTick(); }
function mount(loader: Parameters<typeof useJsonlPreview>[1], initial = 'http://127.0.0.1/a') {
  const url = ref(initial);
  let state!: ReturnType<typeof useJsonlPreview>;
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(defineComponent({setup() { state = useJsonlPreview(url, loader); return () => h('div'); }})); app.mount(host);
  return {state, url};
}
afterEach(() => { app?.unmount(); app = undefined; document.body.innerHTML = ''; });

describe('JSONL incremental loading', () => {
  it('loads ten rows, suppresses concurrent requests, and stops at EOF', async () => {
    const first = deferred<JsonlPage>();
    const loader = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValueOnce(page(11, 3, null));
    const {state} = mount(loader);
    await state.loadMore(); await state.loadMore();
    expect(loader).toHaveBeenCalledTimes(1);
    first.resolve(page()); await settle();
    expect(state.lines.value).toHaveLength(10);
    expect(loader.mock.calls[0][1]).toBeNull();
    await state.loadMore();
    expect(loader.mock.calls[1][1]).toBe('next');
    expect(state.lines.value.map(line => line.lineNumber)).toEqual(Array.from({length: 13}, (_, i) => i + 1));
    expect(state.finished.value).toBe(true);
    await state.loadMore(); expect(loader).toHaveBeenCalledTimes(2);
  });
  it('keeps existing rows and retries exactly the failed cursor', async () => {
    const loader = vi.fn().mockResolvedValueOnce(page()).mockRejectedValueOnce(new Error('网络中断')).mockResolvedValueOnce(page(11, 1, null));
    const {state} = mount(loader); await settle();
    await state.loadMore(); expect(state.error.value).toContain('网络中断');
    expect(state.lines.value).toHaveLength(10);
    await state.loadMore(); expect(loader.mock.calls[1][1]).toBe(loader.mock.calls[2][1]);
    expect(state.lines.value).toHaveLength(11); expect(state.error.value).toBe('');
  });
  it('aborts and discards stale pages after switching attachment or closing preview', async () => {
    const old = deferred<JsonlPage>(), current = deferred<JsonlPage>();
    const loader = vi.fn().mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
    const {state, url} = mount(loader);
    const oldSignal = loader.mock.calls[0][2] as AbortSignal;
    url.value = 'http://127.0.0.1/b'; await settle();
    expect(oldSignal.aborted).toBe(true);
    old.resolve(page(1, 10, null)); await settle(); expect(state.lines.value).toHaveLength(0);
    const currentSignal = loader.mock.calls[1][2] as AbortSignal;
    app!.unmount(); app = undefined; expect(currentSignal.aborted).toBe(true);
    current.resolve(page(1, 10, null)); await settle(); expect(state.lines.value).toHaveLength(0);
  });
  it('rejects gaps without appending inconsistent rows', async () => {
    const loader = vi.fn().mockResolvedValueOnce(page()).mockResolvedValueOnce(page(12, 1, null));
    const {state} = mount(loader); await settle(); await state.loadMore();
    expect(state.error.value).toContain('行号不连续'); expect(state.lines.value).toHaveLength(10);
  });
  it('does not request data before an attachment URL is available', async () => {
    const loader = vi.fn().mockResolvedValue(page(1, 0, null));
    const {state, url} = mount(loader, ''); await settle(); expect(loader).not.toHaveBeenCalled();
    url.value = 'http://127.0.0.1/a'; await settle(); expect(state.finished.value).toBe(true);
    expect(state.lines.value).toHaveLength(0);
  });
});
