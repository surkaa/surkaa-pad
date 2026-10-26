// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, shallowReactive, type App} from 'vue';
import {BottomSheet, Dark, Notify, Quasar} from 'quasar';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import type {MenuItemOptions, MenuOptions} from '@tauri-apps/api/menu';
import {createDiaryTableExtensions} from '../editor/tableEditing';
import Component from '../EditToolbar.vue';
import {DEFAULT_EDITOR_TOOLBAR_ORDER} from '../../utils/editorToolbar';
import {DEFAULT_WINDOWS_EDITOR_SHORTCUTS} from '../../utils/editorShortcuts';

const currentPlatform = vi.hoisted(() => ({value: 'windows'}));
const native = vi.hoisted(() => ({create: vi.fn()}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: () => currentPlatform.value}));
vi.mock('@tauri-apps/api/menu', () => ({Menu: {new: native.create}}));
let app: App | undefined;
let editor: Editor | undefined;
let host: HTMLElement;
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick(); }
async function mount(extra: Record<string, unknown> = {}) {
  host = document.createElement('div'); document.body.append(host);
  const props = shallowReactive({view: true, panelOpen: false, editor, ...extra});
  app = createApp({render: () => h(Component, props)});
  app.use(Quasar, {plugins: {BottomSheet, Notify}}); app.mount(host); await settle();
  return props;
}
function listButton() { return host.querySelector<HTMLButtonElement>('[data-list-menu]')!; }
function nativeItems() { return (native.create.mock.lastCall![0] as MenuOptions).items!; }
function nativeItem(label: string) { return nativeItems().find(entry => 'text' in entry && entry.text === label) as MenuItemOptions; }
function sheetItem(label: string) {
  return Array.from(document.querySelectorAll<HTMLElement>('.q-bottom-sheet__item'))
    .find(item => item.textContent?.trim() === label);
}
async function openMenu() {
  listButton().dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
  await settle();
}
function pointer(type: string, options: PointerEventInit = {}, target: EventTarget = listButton()) {
  target.dispatchEvent(new PointerEvent(type, {bubbles: true, cancelable: true,
    pointerType: 'touch', isPrimary: true, pointerId: 1, clientX: 20, clientY: 20, ...options}));
}
function selectText(text: string) {
  let position = -1;
  editor!.state.doc.descendants((node, pos) => { if (node.isText && node.text === text) position = pos; });
  expect(position).toBeGreaterThanOrEqual(0);
  editor!.commands.setTextSelection(position);
}
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks(); currentPlatform.value = 'windows';
  native.create.mockImplementation(async () => ({popup: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined)}));
});
afterEach(() => {
  app?.unmount(); editor?.destroy(); app = undefined; editor = undefined;
  Dark.set(false); document.body.innerHTML = ''; vi.restoreAllMocks(); vi.useRealTimers();
});

describe('list and table toolbar controls', () => {
  it.each([false, true])('offers a sortable list button without opening the extra panel (dark=%s)', async dark => {
    vi.useFakeTimers();
    editor = new Editor({extensions: [StarterKit], content: '<p>第一行</p><p>第二行</p>'});
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(Component, {view: true, panelOpen: false, editor,
      toolbarOrder: ['bulletList', ...DEFAULT_EDITOR_TOOLBAR_ORDER.filter(action => action !== 'bulletList')],
      shortcuts: DEFAULT_WINDOWS_EDITOR_SHORTCUTS});
    app.use(Quasar); app.mount(host); Dark.set(dark); await nextTick();
    const button = host.querySelector<HTMLButtonElement>('.toolbar-header [aria-label="普通列表"]')!;
    expect(button).not.toBeNull();
    expect(host.querySelector('.toolbar-header .tool-btn')).toBe(button);
    expect(button.title).toBe('普通列表（Ctrl+Shift+8） · 右键打开列表操作');
    expect(button.querySelector('.q-icon')?.textContent).toBe('format_list_bulleted');
    editor.commands.setTextSelection({from: 1, to: 8});
    button.click(); await vi.advanceTimersByTimeAsync(100); await nextTick();
    expect(editor.state.doc.child(0).type.name).toBe('bulletList');
    expect(editor.state.doc.child(0).childCount).toBe(2);
    expect(button.classList.contains('is-active')).toBe(true);
    button.click(); await vi.advanceTimersByTimeAsync(100); await nextTick();
    expect(editor.state.doc.child(0).type.name).toBe('paragraph');
    expect(button.classList.contains('is-active')).toBe(false);
    editor.setEditable(false);
    const before = editor.getJSON(); button.click();
    expect(editor.getJSON()).toEqual(before);
  });

  it.each([false, true])('renders and operates in both themes (dark=%s)', async dark => {
    vi.useFakeTimers();
    editor = new Editor({extensions: [StarterKit, ...createDiaryTableExtensions()], content: '<p>第一行</p><p>第二行</p>'});
    const host = document.createElement('div'); document.body.append(host);
    const insertTable = vi.fn();
    app = createApp(Component, {view: true, panelOpen: true, editor, onInsertTable: insertTable});
    app.use(Quasar); app.mount(host); Dark.set(dark); await nextTick();
    const button = (label: string) => Array.from(host.querySelectorAll<HTMLButtonElement>('button'))
      .find(node => node.textContent?.includes(label))!;
    expect(host.querySelector('.more-panel .list-actions')).toBeNull();
    expect(button('普通列表')).toBeUndefined();
    expect(button('编号列表')).toBeUndefined();
    expect(button('降级')).toBeUndefined();
    expect(button('升级')).toBeUndefined();
    editor.commands.setTextSelection({from: 1, to: 8});
    host.querySelector<HTMLButtonElement>('[aria-label="普通列表"]')!.click();
    expect(editor.state.doc.child(0).type.name).toBe('bulletList');
    expect(editor.state.doc.child(0).childCount).toBe(2);
    editor.commands.setContent('<p></p>');
    await nextTick();
    const tableButton = host.querySelector<HTMLButtonElement>('.toolbar-header [aria-label="表格"]')!;
    expect(tableButton.querySelector('.q-icon')?.textContent).toBe('table_chart');
    expect(host.querySelector('.more-panel [label="表格"]')).toBeNull();
    tableButton.click();
    await vi.advanceTimersByTimeAsync(100); await nextTick();
    expect(insertTable).toHaveBeenCalledOnce();
    expect(editor.state.doc.child(0).type.name).toBe('paragraph');
    editor.commands.insertTable({rows: 3, cols: 3, withHeaderRow: true});
    await vi.advanceTimersByTimeAsync(100);
    await nextTick();
    expect(editor.state.doc.child(0).type.name).toBe('table');
    expect(editor.state.doc.child(0).childCount).toBe(3);
    expect(tableButton.disabled).toBe(true);
    expect(button('添加行')).toBeUndefined();
    expect(button('添加列')).toBeUndefined();
  });

  it.each([false, true])('opens a grouped native menu without editing, and restores the original selection (dark=%s)', async dark => {
    Dark.set(dark);
    editor = new Editor({extensions: [StarterKit], content: '<p>第一行</p><p>第二行</p><p>不选中</p>'});
    await mount();
    editor.commands.setTextSelection({from: 1, to: 8});
    const before = editor.getJSON();
    const selection = editor.state.selection;
    const down = new MouseEvent('mousedown', {bubbles: true, cancelable: true, button: 2});
    listButton().dispatchEvent(down);
    expect(down.defaultPrevented).toBe(true);
    await openMenu();
    expect(editor.getJSON()).toEqual(before);
    expect(editor.state.selection.eq(selection)).toBe(true);
    expect(nativeItems().map(item => 'text' in item ? item.text : 'item' in item ? item.item : null))
      .toEqual(['普通列表', '编号列表', 'Separator', '升级', '降级']);
    expect(nativeItem('升级').enabled).toBe(false);
    expect(nativeItem('降级').enabled).toBe(false);
    editor.commands.setTextSelection(12); // 菜单抢焦点时只改变选区，不应改变目标。
    nativeItem('编号列表').action!(''); await settle();
    expect(editor.state.doc.child(0).type.name).toBe('orderedList');
    expect(editor.state.doc.child(0).childCount).toBe(2);
    expect(editor.state.doc.child(1).textContent).toBe('不选中');
  });

  it('disables unavailable indentation, and supports indent/outdent of the selected list item', async () => {
    editor = new Editor({extensions: [StarterKit], content: '<ul><li><p>第一项</p></li><li><p>第二项</p></li></ul>'});
    await mount(); selectText('第一项'); await openMenu();
    expect(nativeItem('降级').enabled).toBe(false);
    selectText('第二项'); await openMenu();
    expect(nativeItem('降级').enabled).toBe(true);
    nativeItem('降级').action!(''); await settle();
    expect(editor.state.doc.child(0).childCount).toBe(1);
    expect(editor.state.doc.child(0).child(0).child(1).child(0).textContent).toBe('第二项');
    await openMenu(); expect(nativeItem('升级').enabled).toBe(true);
    nativeItem('升级').action!(''); await settle();
    expect(editor.state.doc.child(0).childCount).toBe(2);
  });

  it.each(['ContextMenu', 'F10'])('supports the keyboard menu key: %s', async key => {
    editor = new Editor({extensions: [StarterKit], content: '<p>正文</p>'}); await mount();
    const event = new KeyboardEvent('keydown', {key, shiftKey: key === 'F10', bubbles: true, cancelable: true});
    listButton().dispatchEvent(event); await settle();
    expect(event.defaultPrevented).toBe(true);
    expect(native.create).toHaveBeenCalledOnce();
    expect(editor.state.doc.child(0).type.name).toBe('paragraph');
  });

  it.each(['changed', 'readonly', 'hidden', 'unmounted'])('rejects a stale menu action when %s', async state => {
    editor = new Editor({extensions: [StarterKit], content: '<p>原文</p>'});
    const props = await mount(); await openMenu();
    const notify = vi.spyOn(app!.config.globalProperties.$q, 'notify');
    const action = nativeItem('编号列表').action!;
    if (state === 'changed') editor.commands.setContent('<p>新的正文</p>');
    if (state === 'readonly') editor.setEditable(false);
    if (state === 'hidden') props.view = false;
    if (state === 'unmounted') { app!.unmount(); app = undefined; }
    const before = editor.getJSON();
    action(''); await settle();
    expect(editor.getJSON()).toEqual(before);
    if (state === 'changed') expect(notify).toHaveBeenCalledWith({type: 'warning', message: '日记内容已经发生变化，请重新打开列表菜单'});
  });

  it('does not open menus on other toolbar buttons or in readonly mode', async () => {
    editor = new Editor({extensions: [StarterKit], content: '<p>正文</p>'}); await mount();
    host.querySelector('[aria-label="加粗"]')!.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true}));
    await settle(); expect(native.create).not.toHaveBeenCalled();
    editor.setEditable(false); await settle(); await openMenu();
    expect(native.create).not.toHaveBeenCalled();
  });

  it.each([false, true])('uses the themed mobile menu on long press, without toggling on release (dark=%s)', async dark => {
    currentPlatform.value = 'android'; Dark.set(dark);
    editor = new Editor({extensions: [StarterKit], content: '<ul><li><p>第一项</p></li><li><p>第二项</p></li></ul>'});
    await mount(); selectText('第二项');
    const before = editor.getJSON();
    const blur = vi.spyOn(editor.view.dom, 'blur');
    pointer('pointerdown', {}, listButton().querySelector('.q-icon')!);
    await vi.advanceTimersByTimeAsync(500); await settle();
    expect(blur).toHaveBeenCalled();
    expect(native.create).not.toHaveBeenCalled();
    const sheet = document.querySelector<HTMLElement>('.q-bottom-sheet')!;
    expect(sheet).not.toBeNull();
    expect(sheet.classList.contains('q-bottom-sheet--dark')).toBe(dark);
    expect(sheet.style.background).toBe('var(--pad-bg-color-200)');
    expect(sheet.style.color).toBe('var(--pad-text-color-100)');
    expect(sheet.querySelectorAll('.q-separator')).toHaveLength(1);
    expect(Array.from(sheet.querySelectorAll('.q-bottom-sheet__item'), item => item.textContent?.trim()))
      .toEqual(['普通列表', '编号列表', '升级', '降级']);
    listButton().dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
    await settle();
    expect(document.querySelectorAll('.q-bottom-sheet')).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(3000); pointer('pointerup'); listButton().click(); await settle();
    expect(editor.getJSON()).toEqual(before);
    sheetItem('降级')!.click(); await settle();
    expect(editor.state.doc.child(0).childCount).toBe(1);
    expect(document.querySelector('.q-bottom-sheet')).toBeNull();
    const afterIndent = editor.getJSON();
    pointer('pointerdown'); pointer('pointerup'); listButton().click(); await settle();
    expect(editor.getJSON()).not.toEqual(afterIndent); // 新的一次轻触仍能正常执行，不被长按吞掉。
  });

  it.each(['move', 'release', 'cancel', 'scroll', 'second-finger', 'unmount'])('cancels a mobile long press on %s', async reason => {
    currentPlatform.value = 'android';
    editor = new Editor({extensions: [StarterKit], content: '<p>正文</p>'}); await mount();
    pointer('pointerdown');
    if (reason === 'move') pointer('pointermove', {clientX: 40}, window);
    if (reason === 'release') pointer('pointerup');
    if (reason === 'cancel') pointer('pointercancel');
    if (reason === 'scroll') window.dispatchEvent(new Event('scroll'));
    if (reason === 'second-finger') pointer('pointerdown', {pointerId: 2, isPrimary: false});
    if (reason === 'unmount') { app!.unmount(); app = undefined; }
    await vi.advanceTimersByTimeAsync(600); await settle();
    expect(document.querySelector('.q-bottom-sheet')).toBeNull();
    expect(native.create).not.toHaveBeenCalled();
    expect(editor.state.doc.child(0).type.name).toBe('paragraph');
  });

  it('keeps mobile light taps as ordinary list toggles and hides unavailable menu actions', async () => {
    currentPlatform.value = 'android';
    editor = new Editor({extensions: [StarterKit], content: '<p>正文</p>'}); await mount();
    pointer('pointerdown'); pointer('pointerup'); listButton().click(); await settle();
    expect(editor.state.doc.child(0).type.name).toBe('bulletList');
    expect(document.querySelector('.q-bottom-sheet')).toBeNull();
    pointer('pointerdown'); await vi.advanceTimersByTimeAsync(500); await settle();
    expect(sheetItem('降级')).toBeUndefined(); // 第一个列表项不能降级。
    expect(sheetItem('升级')).toBeDefined();
  });
});
