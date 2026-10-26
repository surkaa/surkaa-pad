// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {createPinia} from 'pinia';
import {BottomSheet, Dark, Notify, Quasar} from 'quasar';
import type {MenuItemOptions, MenuOptions} from '@tauri-apps/api/menu';
import type {Editor} from '@tiptap/vue-3';
import Component from '../TiptapEditor.vue';
import {copyTextToClipboard} from '../../utils/clipboard';

const currentPlatform = vi.hoisted(() => ({value: 'windows'}));
const native = vi.hoisted(() => ({create: vi.fn()}));
vi.mock('@tauri-apps/api/menu', () => ({Menu: {new: native.create}}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: () => currentPlatform.value}));
vi.mock('../../utils/clipboard', () => ({copyTextToClipboard: vi.fn().mockResolvedValue(undefined)}));
let app: App | undefined;
let host: HTMLElement;
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(550); await nextTick(); }
async function mount(text = '前面\n\n后面', extra: Record<string, unknown> = {}) {
  host = document.createElement('div'); document.body.append(host);
  const update = vi.fn();
  app = createApp(Component, {
    modelValue: {nodes: [{type: 'markdown', text}]}, attachments: [], attachmentMap: {},
    'onUpdate:modelValue': update,
    ...extra,
  });
  app.use(createPinia()); app.use(Quasar, {plugins: {Notify, BottomSheet}});
  const exposed = app.mount(host) as unknown as {editor: Editor; openTableDialog: () => void};
  await settle();
  return {...exposed, update};
}
function item(label: string) {
  return Array.from(document.querySelectorAll<HTMLElement>('.q-bottom-sheet .q-item'))
    .find(element => element.textContent?.trim() === label)!;
}
function nativeItems() { return (native.create.mock.lastCall![0] as MenuOptions).items!; }
function nativeItem(label: string) { return nativeItems().find(entry => 'text' in entry && entry.text === label) as MenuItemOptions; }
async function selectAction(label: string, platform: string) {
  if (platform === 'windows') nativeItem(label).action!('');
  else item(label).click();
  await settle();
}
beforeEach(() => {
  vi.useFakeTimers(); localStorage.clear(); sessionStorage.clear(); vi.clearAllMocks();
  native.create.mockImplementation(async () => ({popup: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined)}));
});
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false); currentPlatform.value = 'windows';
  document.body.innerHTML = ''; vi.useRealTimers();
});

describe.each([false, true])('editor table integration (dark=%s)', dark => {
  it.each(['windows', 'android'])('readonly keeps copy/preview/download but no editing actions (%s)', async platform => {
    currentPlatform.value = platform; Dark.set(dark);
    const {editor, update, openTableDialog} = await mount('正文', {
      readOnly: true,
      modelValue: {nodes: [{type: 'file', attachmentId: 'file-1'}, {type: 'summary', summary: '外显', content: '内部文字'}]},
      attachments: [{id: 'file-1', filename: 'test.txt', mimetype: 'text/plain', size: 4,
        encrypted: false, nonce: [], algorithm: 'AES-256-CTR'}],
    });
    expect(editor.isEditable).toBe(false);
    expect(host.querySelector('.is-read-only')).not.toBeNull();
    openTableDialog(); await settle();
    expect(document.querySelector('.table-insert-dialog')).toBeNull();
    host.querySelector('.editor-summary-edit')!.dispatchEvent(new MouseEvent('click', {bubbles: true}));
    await settle();
    expect(document.querySelector('.summary-editor-dialog')).toBeNull();
    host.querySelector('.editor-file-attachment .file-name')!
      .dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
    await settle();
    if (platform === 'windows') {
      expect(nativeItems().map(entry => 'text' in entry ? entry.text : null)).toEqual(['预览', '保存到本地', '批量下载附件']);
    } else {
      expect(Array.from(document.querySelectorAll('.q-bottom-sheet__item'), element => element.textContent?.trim()))
        .toEqual(['预览', '保存到本地', '批量下载附件']);
    }
    await selectAction('批量下载附件', platform);
    expect(document.querySelector('.attachment-batch-dialog')).not.toBeNull();
    expect(document.querySelector('.attachment-batch-dialog [role="checkbox"]')?.getAttribute('aria-checked')).toBe('true');
    expect(update).not.toHaveBeenCalled();
  });

  it('inserts the chosen size at the original cursor and emits saved Markdown', async () => {
    Dark.set(dark);
    const {editor, openTableDialog, update} = await mount();
    editor.commands.setTextSelection(4);
    const before = editor.getJSON();
    openTableDialog(); await settle();
    const inputs = document.querySelectorAll<HTMLInputElement>('.table-insert-dialog input');
    inputs[0].value = '4'; inputs[0].dispatchEvent(new Event('input', {bubbles: true}));
    inputs[1].value = '2'; inputs[1].dispatchEvent(new Event('input', {bubbles: true}));
    await settle();
    Array.from(document.querySelectorAll<HTMLButtonElement>('.table-insert-dialog button'))
      .find(element => element.textContent?.trim() === '插入')!.click();
    await settle();
    const rows = host.querySelectorAll('tr');
    expect(rows).toHaveLength(4); expect(rows[0].children).toHaveLength(2);
    expect(host.querySelectorAll('th')).toHaveLength(2);
    expect(update).toHaveBeenCalled();
    expect(JSON.stringify(update.mock.lastCall![0])).toContain('| --- | --- |');
    expect(editor.state.doc.textContent).toBe('前面后面');
    expect(editor.state.doc.textContent).not.toContain('表头');
    editor.commands.undo(); expect(editor.getJSON()).toEqual(before);
    expect(document.querySelector('.table-insert-dialog')).toBeNull();
  });

  it.each(['windows', 'android'])('opens the targeted cell menu and copies/edits without focusing (%s)', async platform => {
    currentPlatform.value = platform; Dark.set(dark);
    const {editor, update} = await mount('| A | B |\n| --- | --- |\n| 1 | 2 |');
    const cell = host.querySelectorAll<HTMLElement>('td')[1];
    const open = async () => {
      if (platform === 'android') {
        cell.dispatchEvent(new PointerEvent('pointerdown', {bubbles: true, pointerType: 'touch', isPrimary: true, pointerId: 1}));
      } else {
        cell.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
      }
      await settle();
    };
    await open();
    expect(document.querySelector('.table-cell-menu')).toBeNull();
    if (platform === 'windows') {
      expect(nativeItem('当前列右移').enabled).toBe(false);
      expect(nativeItems().filter(entry => 'item' in entry && entry.item === 'Separator')).toHaveLength(3);
    } else {
      expect(document.querySelector('.q-bottom-sheet')).not.toBeNull();
      expect(item('当前列右移')).toBeUndefined();
      expect(document.querySelectorAll('.q-bottom-sheet .q-separator')).toHaveLength(3);
      expect(item('删除整个表格').style.color).toBe('var(--pad-danger-color)');
    }
    await selectAction('复制单元格内容', platform);
    expect(copyTextToClipboard).toHaveBeenCalledExactlyOnceWith('2');
    await open();
    const focus = vi.spyOn(editor.view, 'focus');
    await selectAction('清空当前列', platform);
    expect(focus).not.toHaveBeenCalled();
    expect(Array.from(host.querySelectorAll('tr'), row => Array.from(row.children, cell => cell.textContent)))
      .toEqual([['A', ''], ['1', '']]);
    expect(update).toHaveBeenCalled();
  });

  it('rejects a native menu action after the underlying document changed', async () => {
    const {editor} = await mount('| A | B |\n| --- | --- |\n| 1 | 2 |');
    host.querySelector('td')!.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
    await settle();
    const oldAction = nativeItem('删除整个表格').action!;
    editor.commands.insertContent('更新');
    const before = editor.getJSON();
    oldAction(''); await settle();
    expect(editor.getJSON()).toEqual(before);
    expect(host.querySelector('table')).not.toBeNull();
  });

  it.each(['windows', 'android'])('keeps attachment actions working with the same platform container (%s)', async platform => {
    currentPlatform.value = platform; Dark.set(dark);
    const preview = vi.fn();
    const {editor} = await mount('正文', {attachments: [{
      id: 'file-1', filename: 'test.txt', mimetype: 'text/plain', size: 4,
      encrypted: false, nonce: [], algorithm: 'AES-256-CTR',
    }], onPreviewAttachment: preview});
    editor.commands.insertFile({id: 'file-1', filename: 'test.txt'}); await settle();
    host.querySelector('.editor-file-attachment .file-name')!
      .dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
    await settle();
    const labels = ['预览', '转成加密附件', '保存到本地', '批量下载附件', '重命名附件'];
    if (platform === 'windows') {
      expect(nativeItems().map(entry => 'text' in entry ? entry.text : null)).toEqual(labels);
    } else {
      expect(Array.from(document.querySelectorAll('.q-bottom-sheet__item'), element => element.textContent?.trim())).toEqual(labels);
      expect(document.querySelectorAll('.q-bottom-sheet .q-separator')).toHaveLength(0);
    }
    await selectAction('预览', platform);
    expect(preview).toHaveBeenCalledExactlyOnceWith('file-1');
  });

  it('does not insert a table over content changed while the dialog was open', async () => {
    Dark.set(dark);
    const {editor, openTableDialog} = await mount();
    openTableDialog(); await settle();
    editor.commands.insertContent('更新');
    Array.from(document.querySelectorAll<HTMLButtonElement>('.table-insert-dialog button'))
      .find(element => element.textContent?.trim() === '插入')!.click(); await settle();
    expect(host.querySelector('table')).toBeNull();
    expect(editor.state.doc.textContent).toContain('更新');
  });
});
