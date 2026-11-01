// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App} from 'vue';
import {Dark, Quasar, Notify, Dialog, Screen} from 'quasar';
import Component from '../AttachmentPreviewDialog.vue';
import type {ArchiveEntryEvent, ArchivePreviewEvent, AttachmentMeta} from '../../bindings';

const mocks = vi.hoisted(() => ({list: vi.fn(), prepare: vi.fn(), cancel: vi.fn(), release: vi.fn(), open: vi.fn(),
  listen: vi.fn(), unregister: vi.fn(), platform: vi.fn()}));
vi.mock('../../utils/api', () => ({default: {cmdPreviewArchiveAttachment: mocks.list, cmdPrepareArchiveEntry: mocks.prepare,
  cmdCancelTask: mocks.cancel, cmdReleaseArchiveEntry: mocks.release, cmdOpenArchiveEntryHtml: mocks.open}}));
vi.mock('@tauri-apps/api/core', () => ({isTauri: () => true, Channel: class {onmessage = (_message: unknown) => undefined;}}));
vi.mock('@tauri-apps/api/app', () => ({onBackButtonPress: mocks.listen}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: mocks.platform}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl: vi.fn()}));

let app: App | undefined;
const small = Screen.lt.sm;
let entryChannel: {onmessage: (message: ArchiveEntryEvent) => void};
let nativeBack: (payload: {canGoBack: boolean}) => void;
const visible = ref(false);
const settle = async () => {await vi.advanceTimersByTimeAsync(500); await nextTick();};
beforeEach(() => {
  vi.useFakeTimers(); vi.resetAllMocks(); visible.value = true;
  mocks.platform.mockReturnValue('android'); mocks.unregister.mockResolvedValue(undefined);
  mocks.listen.mockImplementation(async callback => {nativeBack = callback; return {unregister: mocks.unregister};});
  mocks.cancel.mockResolvedValue(undefined); mocks.release.mockResolvedValue(undefined);
  mocks.list.mockImplementation(async (channel: {onmessage: (message: ArchivePreviewEvent) => void}) => {
    channel.onmessage({event: 'completed', data: {preview: {format: 'zip', archiveSize: 500, uncompressedSize: 14,
      fileCount: 2, directoryCount: 0, isSolid: false, encrypted: false, revision: 'etag', entries:
        ['folder/document.json', 'folder/page.html'].map(path => ({path, size: 7, compressedSize: 7, isDirectory: false, encrypted: false, modifiedAt: null})),
    }}}); return 'list-task';
  });
  mocks.prepare.mockImplementation(async (_target, _password, channel) => {entryChannel = channel; return 'entry-task';});
  vi.stubGlobal('fetch', vi.fn(async () => new Response('{"v":1}')));
});
afterEach(async () => {
  app?.unmount(); app = undefined; await settle(); Dark.set(false); Screen.lt.sm = small;
  document.body.innerHTML = ''; vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks();
});
async function mount(platform = 'android', dark = false) {
  mocks.platform.mockReturnValue(platform); Screen.lt.sm = platform === 'android';
  const attachment: AttachmentMeta = {id: 'zip', filename: 'test.zip', size: 500, mimetype: 'application/zip',
    encrypted: false, algorithm: 'AES-256-CTR', nonce: [], etag: 'etag', contentInfo: null};
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(defineComponent({setup: () => () => h(Component, {modelValue: visible.value,
    'onUpdate:modelValue': value => {visible.value = value;}, diaryId: 'diary', attachment})}));
  app.use(Quasar, {plugins: {Notify, Dialog}}); Dark.set(dark); app.mount(host); await settle();
}
async function openEntry(name: string, complete = true) {
  [...document.querySelectorAll<HTMLElement>('.archive-row')].find(row => row.textContent?.includes(name))!.click();
  await nextTick();
  if (complete) {
    entryChannel.onmessage({event: 'completed', data: {resource: {id: 'entry', filename: name, size: 7,
      mimetype: name.endsWith('html') ? 'text/html' : 'application/json', url: 'http://localhost/archive-entry/entry'}}});
    await settle();
  }
}
async function pressBack(platform: string) {
  if (platform === 'android') nativeBack({canGoBack: true});
  else {
    window.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', keyCode: 27, bubbles: true}));
    window.dispatchEvent(new KeyboardEvent('keyup', {key: 'Escape', keyCode: 27, bubbles: true}));
  }
  await settle();
}

describe('preview back navigation without leaving the diary route', () => {
  it.each([['android', false], ['android', true], ['windows', false], ['windows', true]] as const)(
    'returns from file to the same directory, then closes (%s, dark=%s)', async (platform, dark) => {
      const history = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
      await mount(platform, dark);
      const tree = document.querySelector<HTMLElement>('.archive-preview')!;
      const search = tree.querySelector<HTMLInputElement>('input')!;
      search.value = 'document'; search.dispatchEvent(new Event('input', {bubbles: true})); await settle();
      const scroller = tree.querySelector<HTMLElement>('.archive-tree')!; scroller.scrollTop = 44;
      await openEntry('document.json'); expect(document.querySelector('.json-tree-viewer')).not.toBeNull();
      await pressBack(platform);
      expect(visible.value).toBe(true); expect(document.querySelector('.archive-preview')).toBe(tree);
      expect(tree.style.display).not.toBe('none'); expect(search.value).toBe('document'); expect(scroller.scrollTop).toBe(44);
      expect(mocks.list).toHaveBeenCalledTimes(1); expect(mocks.release).toHaveBeenCalledWith('entry', false);
      await pressBack(platform); expect(visible.value).toBe(false); expect(history).not.toHaveBeenCalled();
      if (platform === 'android') expect(mocks.unregister).toHaveBeenCalledTimes(1); else expect(mocks.listen).not.toHaveBeenCalled();
    });

  it.each(['android', 'windows'])('cancels extraction on back and discards late completion (%s)', async platform => {
    await mount(platform); await openEntry('document.json', false); await pressBack(platform);
    expect(visible.value).toBe(true); expect(mocks.cancel).toHaveBeenCalledWith('entry-task');
    entryChannel.onmessage({event: 'completed', data: {resource: {id: 'late', filename: 'document.json', size: 7,
      mimetype: 'application/json', url: 'http://localhost/archive-entry/late'}}}); await settle();
    expect(mocks.release).toHaveBeenCalledWith('late', true); expect(document.querySelector('.json-tree-viewer')).toBeNull();
  });

  it.each(['android', 'windows'])('dismisses HTML confirmation before leaving its preview (%s)', async platform => {
    await mount(platform); await openEntry('page.html');
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent?.includes('在浏览器中打开'))!.click();
    await settle(); expect(document.querySelectorAll('.q-dialog')).toHaveLength(2);
    await pressBack(platform);
    expect(document.querySelectorAll('.q-dialog')).toHaveLength(1); expect(document.body.textContent).toContain('在浏览器中打开');
    expect(document.querySelector('[aria-label="返回压缩包目录"]')).not.toBeNull(); expect(mocks.open).not.toHaveBeenCalled();
    await pressBack(platform); expect(visible.value).toBe(true); expect(document.querySelector('[aria-label="返回压缩包目录"]')).toBeNull();
    await pressBack(platform); expect(visible.value).toBe(false);
  });

  it('keeps the explicit close button as a direct exit from the whole preview', async () => {
    await mount(); await openEntry('document.json');
    document.querySelector<HTMLElement>('[aria-label="关闭附件预览"]')!.click(); await settle();
    expect(visible.value).toBe(false); expect(mocks.release).toHaveBeenCalledWith('entry', true); expect(mocks.unregister).toHaveBeenCalledTimes(1);
  });

  it('also removes the nested confirmation when its parent preview is closed externally', async () => {
    await mount(); await openEntry('page.html');
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent?.includes('在浏览器中打开'))!.click();
    await settle(); visible.value = false; await settle();
    expect(document.querySelectorAll('.q-dialog')).toHaveLength(0); expect(mocks.open).not.toHaveBeenCalled();
    expect(mocks.unregister).toHaveBeenCalledTimes(1);
  });
});
