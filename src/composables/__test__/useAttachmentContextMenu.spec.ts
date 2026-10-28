// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, shallowRef, type App} from 'vue';
import {BottomSheet, Dark, Notify, Quasar} from 'quasar';
import type {Editor} from '@tiptap/vue-3';
import type {MenuItemOptions, MenuOptions} from '@tauri-apps/api/menu';
import type {AttachmentMeta} from '../../bindings';
import {useAttachmentContextMenu} from '../useAttachmentContextMenu';

const native = vi.hoisted(() => ({create: vi.fn()}));
vi.mock('@tauri-apps/api/menu', () => ({Menu: {new: native.create}}));
let app: App | undefined;
const rotations = ['顺时针旋转90°', '逆时针旋转90°', '旋转180°'];
const common = ['转成普通附件', '保存到本地', '批量下载附件'];

async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick(); }
async function open(platform: string, dark: boolean, {
  hasAlbum = true, albumImage = false, readOnly = false, type = 'image',
}: {hasAlbum?: boolean; albumImage?: boolean; readOnly?: boolean; type?: 'image' | 'file' | 'audio' | 'video'} = {}) {
  const host = document.createElement('div'); document.body.append(host);
  const boundary = document.createElement('div'); document.body.append(boundary);
  const target = document.createElement(type === 'image' ? 'img' : type === 'file' ? 'div' : type);
  target.dataset.id = 'attachment';
  if (type === 'file') target.className = 'editor-file-attachment';
  if (albumImage) {
    const album = document.createElement('div'); album.className = 'editor-image-album';
    album.dataset.id = 'album'; album.dataset.displayMode = 'stackedCards';
    boundary.append(album); album.append(target);
  } else boundary.append(target);
  const rotate = vi.fn(); const createAlbum = vi.fn(); const joinAlbum = vi.fn();
  const attachment = {id: 'attachment', filename: type === 'file' ? 'example.pdf' : 'example.jpg',
    mimetype: type === 'file' ? 'application/pdf' : 'image/jpeg', encrypted: true} as AttachmentMeta;
  let handle!: ReturnType<typeof useAttachmentContextMenu>['handleContextMenu'];
  app = createApp(defineComponent({setup() {
    const editor = shallowRef({isEditable: !readOnly, getJSON: () => ({type: 'doc', content: hasAlbum
      ? [{type: 'albumNode', attrs: {id: 'album', images: ['other']}}] : []})} as unknown as Editor);
    ({handleContextMenu: handle} = useAttachmentContextMenu({
      editor, editorElement: ref(boundary), currentPlatform: platform,
      getAttachment: () => attachment, attachmentUrl: () => 'blob:test',
      albumActions: {startAlbumSelection: createAlbum, openAlbumInsertDialog: joinAlbum,
        changeAlbumMode: vi.fn(), applyAlbumSplit: vi.fn(), requestSingleImageSplit: vi.fn(), requestRangeSplit: vi.fn()},
      rotate, toggleEncryption: vi.fn(), rename: vi.fn(), saveDecrypted: vi.fn(), startBatchDownload: vi.fn(),
      openHtml: vi.fn(), previewAttachment: vi.fn(), showImage: vi.fn(),
    }));
    return () => h('div');
  }}));
  app.use(Quasar, {plugins: {BottomSheet, Notify}}); Dark.set(dark); app.mount(host);
  const event = new MouseEvent('contextmenu', {bubbles: true, cancelable: true});
  target.dispatchEvent(event); await handle(event); await settle();
  return {rotate, createAlbum, joinAlbum};
}
function nativeItems() { return (native.create.mock.lastCall![0] as MenuOptions).items!; }
function nativeLabels() { return nativeItems().map(item => 'text' in item ? item.text : 'item' in item ? item.item : undefined); }
function sheetItems() { return [...document.querySelectorAll<HTMLElement>('.q-bottom-sheet__item')]; }

beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks();
  native.create.mockResolvedValue({popup: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined)});
});
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers();
});

describe('standalone image menu groups', () => {
  it.each([false, true])('separates native rotation and album groups without changing callbacks, dark=%s', async dark => {
    const actions = await open('windows', dark);
    expect(nativeLabels()).toEqual([...common, '小图模式', 'Separator', ...rotations, 'Separator', '创建图集', '加入已有图集']);
    for (let index = 0; index < rotations.length; index += 1) {
      const item = nativeItems().find(item => 'text' in item && item.text === rotations[index]) as MenuItemOptions;
      item.action!(''); await settle();
      expect(actions.rotate).toHaveBeenNthCalledWith(index + 1, 'attachment', [90, -90, 180][index]);
    }
    (nativeItems().find(item => 'text' in item && item.text === '创建图集') as MenuItemOptions).action!('');
    (nativeItems().find(item => 'text' in item && item.text === '加入已有图集') as MenuItemOptions).action!('');
    await settle();
    expect(actions.createAlbum).toHaveBeenCalledWith('attachment'); expect(actions.joinAlbum).toHaveBeenCalledWith('attachment');
  });
  it.each([false, true])('renders two separators in a themed Android BottomSheet, dark=%s', async dark => {
    const actions = await open('android', dark);
    expect(native.create).not.toHaveBeenCalled();
    const sheet = document.querySelector<HTMLElement>('.q-bottom-sheet')!;
    expect(sheet.classList.contains('q-bottom-sheet--dark')).toBe(dark);
    expect(sheet.style.background).toBe('var(--pad-bg-color-200)');
    expect(sheet.style.color).toBe('var(--pad-text-color-100)');
    expect(sheet.querySelectorAll('.q-separator')).toHaveLength(2);
    expect(sheetItems().map(item => item.textContent?.trim())).toEqual([...common, '小图模式', ...rotations, '创建图集', '加入已有图集']);
    sheetItems().find(item => item.textContent?.trim() === '加入已有图集')!.click(); await settle();
    expect(actions.joinAlbum).toHaveBeenCalledWith('attachment');
  });
  it('keeps a single create-album item in its own group when no albums exist', async () => {
    await open('windows', false, {hasAlbum: false});
    expect(nativeLabels()).toEqual([...common, '小图模式', 'Separator', ...rotations, 'Separator', '创建图集']);
  });
  it('preserves the existing flat menu for images inside albums', async () => {
    await open('windows', false, {albumImage: true});
    expect(nativeLabels()).toEqual([...common, ...rotations, '预览图片', '切换为横向图集', '拆分整个图集', '仅拆分当前图片', '拆分当前及前后图片']);
  });
  it('does not expose editing groups for read-only images', async () => {
    await open('windows', true, {readOnly: true});
    expect(nativeLabels()).toEqual(['保存到本地', '批量下载附件']);
  });
  it.each(['file', 'audio', 'video'] as const)('does not change the %s menu', async type => {
    await open('windows', false, {type});
    expect(nativeLabels()).toEqual(type === 'file' ? ['预览', ...common, '重命名附件'] : common);
  });
});
