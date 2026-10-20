// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Notify, Quasar} from 'quasar';
import Component from '../JsonlAttachmentPreview.vue';
import {copyTextToClipboard} from '../../utils/clipboard';
import {JSONL_ROW_MIN_HEIGHT} from '../../utils/jsonlPreview';

vi.mock('../../utils/clipboard', () => ({copyTextToClipboard: vi.fn().mockResolvedValue(undefined)}));
let app: App | undefined;
const fetchMock = vi.fn();
const layout = {height: 400, width: 800, reportedHeight: 0};
const observers = new Map<Element, Set<() => void>>();
const rowHeights = new Map<Element, number>();
const records = (start = 1, nextCursor: string | null = 'next') => ({lines: Array.from({length: 10}, (_, i) => ({
  lineNumber: start + i, text: i === 1 ? '{bad}' : JSON.stringify({id: start + i}), error: null,
})), nextCursor});
async function settle(ms = 300) { await vi.advanceTimersByTimeAsync(ms); await nextTick(); }
async function mount(dark = false) {
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(Component, {url: 'http://127.0.0.1/token/diary/file'});
  app.use(Quasar, {plugins: {Notify}}).mount(host); Dark.set(dark); await settle();
}
beforeEach(() => {
  vi.useFakeTimers(); fetchMock.mockReset().mockImplementation(async () => new Response(JSON.stringify(records())));
  vi.stubGlobal('fetch', fetchMock); vi.mocked(copyTextToClipboard).mockClear();
  Object.assign(layout, {height: 400, width: 800, reportedHeight: 0});
  observers.clear(); rowHeights.clear();
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
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return this.classList.contains('jsonl-scroll') ? layout.height : 0;
  });
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains('jsonl-preview')) return layout.height + 84;
    return this.classList.contains('jsonl-row') ? rowHeights.get(this) ?? JSONL_ROW_MIN_HEIGHT : 0;
  });
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockImplementation(() => layout.width);
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockImplementation(function (this: HTMLElement) {
    if (!this.classList.contains('jsonl-scroll')) return 0;
    // happy-dom 无布局引擎：模拟 CSS min-height 在虚拟占位重测期间保留的总高度。
    const virtual = this.querySelector<HTMLElement>('.q-virtual-scroll');
    return Math.max(layout.reportedHeight, Number.parseFloat(virtual?.style.minHeight || '0'));
  });
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    const scroll = this.closest<HTMLElement>('.jsonl-scroll');
    return new DOMRect(0, this.classList.contains('q-virtual-scroll') ? -(scroll?.scrollTop ?? 0) : 0,
      layout.width, this.classList.contains('jsonl-scroll') ? layout.height : this.offsetHeight);
  });
});
afterEach(() => { app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

function mockManyPages(total = 200) {
  fetchMock.mockImplementation(async (url: string) => {
    const start = Number(new URL(url).searchParams.get('cursor') ?? 0) + 1;
    return new Response(JSON.stringify(records(start, start + 9 >= total ? null : String(start + 9))));
  });
}
function notifyResize(element: Element) { for (const notify of observers.get(element) ?? []) notify(); }

describe('JSONL preview list', () => {
  it('shows an empty file and never starts another page', async () => {
    fetchMock.mockResolvedValueOnce(new Response('{"lines":[],"nextCursor":null}'));
    await mount(); await settle(1000);
    expect(document.querySelector('.jsonl-empty')?.textContent).toBe('文件为空');
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
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
    layout.reportedHeight = 1200;
    scroll.scrollTop = 0; scroll.dispatchEvent(new Event('scroll')); await settle();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    scroll.scrollTop = 800; scroll.dispatchEvent(new Event('scroll')); scroll.dispatchEvent(new Event('scroll')); await settle();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(new URL(fetchMock.mock.calls[1][0]).searchParams.get('cursor')).toBe('next');
    expect(document.querySelector('.jsonl-toolbar')?.textContent).toContain('已读取 20 行');
    expect(document.querySelector('.jsonl-status')?.textContent).toContain('已到文件末尾');
  });

  it('does not drain future pages when virtual height temporarily shrinks or rows resize', async () => {
    mockManyPages(); await mount();
    const scroll = document.querySelector<HTMLElement>('.jsonl-scroll')!;
    scroll.scrollTop = 640;
    scroll.dispatchEvent(new Event('scroll')); await settle();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    layout.reportedHeight = 400; // 模拟虚拟切片重测时，物理内容暂时只有一屏高。
    for (let i = 0; i < 5; i++) {
      scroll.dispatchEvent(new Event('scroll'));
      layout.width++;
      notifyResize(document.querySelector('.jsonl-preview')!);
      const row = document.querySelector('.jsonl-row')!;
      rowHeights.set(row, 104 + (i % 2) * 200); notifyResize(row);
      await settle();
    }
    await settle(2000);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(document.querySelector('.jsonl-toolbar')?.textContent).toContain('已读取 20 行');
    scroll.scrollTop = 0; scroll.dispatchEvent(new Event('scroll')); await settle();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    scroll.scrollTop = 1680; scroll.dispatchEvent(new Event('scroll')); await settle();
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('fills a tall viewport by bounded pages and then stops without reaching EOF', async () => {
    layout.height = 3000; mockManyPages(); await mount(); await settle(2000);
    expect(fetchMock).toHaveBeenCalledTimes(4);
    expect(document.querySelector('.jsonl-toolbar')?.textContent).toContain('已读取 40 行');
    await settle(2000); expect(fetchMock).toHaveBeenCalledTimes(4);
  });

  it('does not auto-load a hidden viewport and stops on error until explicit retry', async () => {
    layout.height = 0; mockManyPages(); await mount(); await settle(1000);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    layout.height = 400; notifyResize(document.querySelector('.jsonl-preview')!); await settle();
    fetchMock.mockResolvedValueOnce(new Response('读取失败', {status: 500}));
    const scroll = document.querySelector<HTMLElement>('.jsonl-scroll')!;
    scroll.scrollTop = 640; scroll.dispatchEvent(new Event('scroll')); await settle(1000);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(document.querySelector('.jsonl-error')?.textContent).toContain('读取失败');
    scroll.dispatchEvent(new Event('scroll')); await settle(1000);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(button => button.textContent?.includes('重试'))!.click();
    await settle(1000); expect(fetchMock).toHaveBeenCalledTimes(3);
  });
});
