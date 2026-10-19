// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Notify, Quasar} from 'quasar';
import Component from '../JsonlAttachmentPreview.vue';
import {copyTextToClipboard} from '../../utils/clipboard';

vi.mock('../../utils/clipboard', () => ({copyTextToClipboard: vi.fn().mockResolvedValue(undefined)}));
let app: App | undefined;
const fetchMock = vi.fn();
const records = (start = 1, nextCursor: string | null = 'next') => ({lines: Array.from({length: 10}, (_, i) => ({
  lineNumber: start + i, text: i === 1 ? '{bad}' : JSON.stringify({id: start + i}), error: null,
})), nextCursor});
async function settle() { await vi.advanceTimersByTimeAsync(300); await nextTick(); }
async function mount(dark = false) {
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(Component, {url: 'http://127.0.0.1/token/diary/file'});
  app.use(Quasar, {plugins: {Notify}}).mount(host); Dark.set(dark); await settle();
}
beforeEach(() => {
  vi.useFakeTimers(); fetchMock.mockReset().mockImplementation(async () => new Response(JSON.stringify(records())));
  vi.stubGlobal('fetch', fetchMock); vi.mocked(copyTextToClipboard).mockClear();
});
afterEach(() => { app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('JSONL preview list', () => {
  it.each([false, true])('renders independent records and supports raw copying in both themes (dark=%s)', async dark => {
    await mount(dark);
    expect(document.querySelector('.jsonl-toolbar')?.textContent).toContain('已读取 10 行');
    expect(document.querySelector('.jsonl-row-warning')?.textContent).toContain('JSON 格式错误');
    expect(fetchMock).toHaveBeenCalledTimes(1);
    document.querySelector<HTMLButtonElement>('[aria-label="复制第 1 行"]')!.click(); await settle();
    expect(copyTextToClipboard).toHaveBeenCalledWith('{"id":1}');
    document.querySelector<HTMLElement>('[data-line="1"] .q-btn')!.click(); await settle();
    expect(document.querySelector('.json-tree-viewer')).not.toBeNull();
    expect(Dark.isActive).toBe(dark);
  });
  it('loads another ten records only near the actual scroll bottom, not just when the virtual slice renders', async () => {
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify(records()))).mockResolvedValueOnce(new Response(JSON.stringify(records(11, null))));
    await mount();
    const scroll = document.querySelector<HTMLElement>('.jsonl-scroll')!;
    Object.defineProperties(scroll, {clientHeight: {value: 400}, scrollHeight: {value: 1200}});
    scroll.scrollTop = 0; scroll.dispatchEvent(new Event('scroll')); await settle();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    scroll.scrollTop = 800; scroll.dispatchEvent(new Event('scroll')); scroll.dispatchEvent(new Event('scroll')); await settle();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(new URL(fetchMock.mock.calls[1][0]).searchParams.get('cursor')).toBe('next');
    expect(document.querySelector('.jsonl-toolbar')?.textContent).toContain('已读取 20 行');
    expect(document.querySelector('.jsonl-status')?.textContent).toContain('已到文件末尾');
  });
});
