// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {createPinia} from 'pinia';
import {Dark, Notify, Quasar} from 'quasar';
import type {Editor} from '@tiptap/vue-3';
import Component from '../TiptapEditor.vue';
import {copyTextToClipboard} from '../../utils/clipboard';

const currentPlatform = vi.hoisted(() => ({value: 'windows'}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: () => currentPlatform.value}));
vi.mock('../../utils/clipboard', () => ({copyTextToClipboard: vi.fn().mockResolvedValue(undefined)}));
let app: App | undefined;
let host: HTMLElement;
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(550); await nextTick(); }
async function mount(text = '前面\n\n后面') {
  host = document.createElement('div'); document.body.append(host);
  const update = vi.fn();
  app = createApp(Component, {
    modelValue: {nodes: [{type: 'markdown', text}]}, attachments: [], attachmentMap: {},
    'onUpdate:modelValue': update,
  });
  app.use(createPinia()); app.use(Quasar, {plugins: {Notify}});
  const exposed = app.mount(host) as unknown as {editor: Editor; openTableDialog: () => void};
  await settle();
  return {...exposed, update};
}
function item(label: string) {
  return Array.from(document.querySelectorAll<HTMLElement>('.table-menu-list .q-item'))
    .find(element => element.textContent?.trim() === label)!;
}
beforeEach(() => { vi.useFakeTimers(); localStorage.clear(); sessionStorage.clear(); vi.clearAllMocks(); });
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false); currentPlatform.value = 'windows';
  document.body.innerHTML = ''; vi.useRealTimers();
});

describe.each([false, true])('editor table integration (dark=%s)', dark => {
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
    expect(document.querySelector('.table-cell-menu')).not.toBeNull();
    expect(item('当前列右移').classList.contains('disabled')).toBe(true);
    item('复制单元格内容').click(); await settle();
    expect(copyTextToClipboard).toHaveBeenCalledExactlyOnceWith('2');
    await open();
    const focus = vi.spyOn(editor.view, 'focus');
    item('清空当前列').click(); await settle();
    expect(focus).not.toHaveBeenCalled();
    expect(Array.from(host.querySelectorAll('tr'), row => Array.from(row.children, cell => cell.textContent)))
      .toEqual([['A', ''], ['1', '']]);
    expect(update).toHaveBeenCalled();
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
