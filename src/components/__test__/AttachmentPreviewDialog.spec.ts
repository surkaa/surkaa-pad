// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App} from 'vue';
import {Dark, Quasar, Notify, Screen} from 'quasar';
import Component from '../AttachmentPreviewDialog.vue';
import type {AttachmentMeta} from '../../bindings';

let app: App | undefined;
const initiallySmall = Screen.lt.sm;
afterEach(() => { app?.unmount(); app = undefined; Dark.set(false); Screen.lt.sm = initiallySmall; document.body.innerHTML = ''; vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('JSONL attachment dialog integration', () => {
  it('uses backend pagination even for huge attachments and aborts loading when closed', async () => {
    vi.useFakeTimers();
    const attachment: AttachmentMeta = {id: 'large', filename: 'large.jsonl', mimetype: 'application/json',
      size: 50 * 1024 ** 3, encrypted: true, nonce: [], algorithm: 'AES-256-CTR', etag: null, contentInfo: null};
    let finish!: (response: Response) => void;
    const fetchMock = vi.fn(() => new Promise<Response>(resolve => { finish = resolve; }));
    vi.stubGlobal('fetch', fetchMock);
    const visible = ref(true);
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(defineComponent({setup: () => () => h(Component, {modelValue: visible.value,
      'onUpdate:modelValue': value => { visible.value = value; }, diaryId: 'diary', attachment,
      url: 'http://127.0.0.1/token/diary/large'})}));
    app.use(Quasar, {plugins: {Notify}}).mount(host);
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, options] = vi.mocked(fetchMock).mock.calls[0] as unknown as [string, RequestInit];
    expect(new URL(url).searchParams.get('view')).toBe('jsonl');
    expect(document.body.textContent).not.toContain('文本附件超过');
    document.querySelector<HTMLElement>('[aria-label="关闭附件预览"]')!.click();
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    expect(options.signal?.aborted).toBe(true);
    finish(new Response('{"lines":[{"lineNumber":1,"text":"{}","error":null}],"nextCursor":null}'));
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    expect(document.querySelector('.jsonl-preview')).toBeNull();
  });
});

describe('JSON attachment fullscreen', () => {
  it.each([false, true])('preserves the loaded attachment when toggling fullscreen (dark=%s)', async dark => {
    vi.useFakeTimers();
    const fetchMock = vi.fn(async () => new Response('{"id":1}'));
    vi.stubGlobal('fetch', fetchMock);
    const attachment: AttachmentMeta = {id: 'json', filename: 'data.json', mimetype: 'application/json',
      size: 8, encrypted: false, nonce: [], algorithm: 'AES-256-CTR', etag: null, contentInfo: null};
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(Component, {modelValue: true, diaryId: 'diary', attachment, url: 'http://127.0.0.1/token/diary/json'});
    app.use(Quasar, {plugins: {Notify}});
    Screen.lt.sm = false; Dark.set(dark); app.mount(host);
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    const tree = document.querySelector('.json-tree-viewer');
    expect(tree).not.toBeNull();
    expect(document.querySelector('.vjs-tree')?.classList.contains('dark')).toBe(dark);
    document.querySelector<HTMLButtonElement>('[aria-label="全屏预览"]')!.click(); await nextTick();
    expect(document.querySelector('.attachment-preview-card--maximized')).not.toBeNull();
    expect(document.querySelector('.json-tree-viewer')).toBe(tree);
    document.querySelector<HTMLButtonElement>('[aria-label="退出全屏"]')!.click(); await nextTick();
    expect(document.querySelector('.attachment-preview-card--maximized')).toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
