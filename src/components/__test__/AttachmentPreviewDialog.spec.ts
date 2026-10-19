// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App} from 'vue';
import {Quasar, Notify} from 'quasar';
import Component from '../AttachmentPreviewDialog.vue';
import type {AttachmentMeta} from '../../bindings';

let app: App | undefined;
afterEach(() => { app?.unmount(); app = undefined; document.body.innerHTML = ''; vi.useRealTimers(); vi.unstubAllGlobals(); });

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
