// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App} from 'vue';
import {Dark, Quasar, Notify, Screen} from 'quasar';
import Component from '../AttachmentPreviewDialog.vue';
import type {ArchiveEntryEvent, ArchivePreviewEvent, AttachmentMeta} from '../../bindings';
const mocks = vi.hoisted(() => ({list: vi.fn(), prepare: vi.fn(), cancel: vi.fn(), release: vi.fn()}));
vi.mock('../../utils/api', () => ({default: {cmdPreviewArchiveAttachment: mocks.list, cmdPrepareArchiveEntry: mocks.prepare,
  cmdCancelTask: mocks.cancel, cmdReleaseArchiveEntry: mocks.release}}));
vi.mock('@tauri-apps/api/core', () => ({Channel: class {onmessage = (_message: unknown) => undefined;}}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl: vi.fn()}));
let app: App | undefined;
const small = Screen.lt.sm;
afterEach(() => {app?.unmount(); app = undefined; Dark.set(false); Screen.lt.sm = small; document.body.innerHTML = ''; vi.useRealTimers(); vi.unstubAllGlobals(); vi.clearAllMocks();});

describe('archive navigation', () => {
  it.each([false, true])('keeps directory state and uses the original entry index (dark/mobile=%s)', async dark => {
    vi.useFakeTimers(); Screen.lt.sm = dark; Dark.set(dark);
    let entryChannel!: {onmessage: (message: ArchiveEntryEvent) => void};
    mocks.cancel.mockResolvedValue(undefined); mocks.release.mockResolvedValue(undefined);
    mocks.list.mockImplementation(async (channel: {onmessage: (message: ArchivePreviewEvent) => void}) => {
      channel.onmessage({event: 'completed', data: {preview: {format: 'zip', archiveSize: 500, uncompressedSize: 7, fileCount: 2,
        directoryCount: 0, isSolid: false, encrypted: false, revision: 'etag', entries: [
          {path: 'z.txt', size: 0, compressedSize: 0, isDirectory: false, encrypted: false, modifiedAt: null},
          {path: 'folder/data.json', size: 7, compressedSize: 7, isDirectory: false, encrypted: false, modifiedAt: null},
        ]}}});
      return 'list';
    });
    mocks.prepare.mockImplementation(async (_target, _password, channel) => {entryChannel = channel; return 'prepare';});
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"v":1}')));
    const attachment: AttachmentMeta = {id: 'zip', filename: 'data.zip', size: 500, mimetype: 'application/zip',
      encrypted: false, algorithm: 'AES-256-CTR', nonce: [], etag: 'etag', contentInfo: null};
    const visible = ref(true);
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(defineComponent({setup: () => () => h(Component, {modelValue: visible.value,
      'onUpdate:modelValue': value => {visible.value = value;}, diaryId: 'diary', attachment})}));
    app.use(Quasar, {plugins: {Notify}}).mount(host);
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    const tree = document.querySelector('.archive-preview')!;
    const search = tree.querySelector<HTMLInputElement>('input')!;
    search.value = 'data'; search.dispatchEvent(new Event('input', {bubbles: true}));
    await vi.advanceTimersByTimeAsync(200); await nextTick();
    const row = [...tree.querySelectorAll<HTMLElement>('.archive-row')].find(element => element.textContent?.includes('data.json'))!;
    expect(row).toBeDefined(); row.click(); await nextTick();
    expect(mocks.prepare.mock.calls[0]?.[0]).toMatchObject({index: 1, path: 'folder/data.json', revision: 'etag'});
    expect((tree as HTMLElement).style.display).toBe('none');
    entryChannel.onmessage({event: 'completed', data: {resource: {id: 'entry', filename: 'data.json', size: 7,
      mimetype: 'application/json', url: 'http://localhost/archive-entry/entry'}}});
    await vi.advanceTimersByTimeAsync(200); await nextTick();
    expect(document.querySelector('.json-tree-viewer')).not.toBeNull();
    document.querySelector<HTMLElement>('[aria-label="返回压缩包目录"]')!.click(); await nextTick();
    expect(document.querySelector('.archive-preview')).toBe(tree);
    expect(search.value).toBe('data');
    expect((tree as HTMLElement).style.display).not.toBe('none');
    expect(mocks.release).toHaveBeenCalledWith('entry', false);
    expect(mocks.list).toHaveBeenCalledTimes(1);
    document.querySelector<HTMLElement>('[aria-label="关闭附件预览"]')!.click();
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    expect(mocks.release).toHaveBeenCalledWith('entry', true);
  });
});
