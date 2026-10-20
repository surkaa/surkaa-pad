// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {createDiaryTableExtensions} from '../editor/tableEditing';
import Component from '../EditToolbar.vue';
import {DEFAULT_EDITOR_TOOLBAR_ORDER} from '../../utils/editorToolbar';
import {DEFAULT_WINDOWS_EDITOR_SHORTCUTS} from '../../utils/editorShortcuts';

vi.mock('@tauri-apps/plugin-os', () => ({platform: () => 'windows'}));
let app: App | undefined;
let editor: Editor | undefined;
afterEach(() => {
  app?.unmount(); editor?.destroy(); app = undefined; editor = undefined;
  Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers();
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
    expect(button.title).toBe('普通列表（Ctrl+Shift+8）');
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
    expect(button('普通列表')).toBeDefined();
    expect(button('编号列表')).toBeDefined();
    expect(button('降级')).toBeDefined();
    expect(button('升级')).toBeDefined();
    editor.commands.setTextSelection({from: 1, to: 8});
    button('普通列表').click();
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
});
