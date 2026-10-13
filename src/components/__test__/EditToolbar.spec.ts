// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {createDiaryTableExtensions} from '../editor/tableEditing';
import Component from '../EditToolbar.vue';

vi.mock('@tauri-apps/plugin-os', () => ({platform: () => 'windows'}));
let app: App | undefined;
let editor: Editor | undefined;
afterEach(() => {
  app?.unmount(); editor?.destroy(); app = undefined; editor = undefined;
  Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers();
});

describe('list and table toolbar controls', () => {
  it.each([false, true])('renders and operates in both themes (dark=%s)', async dark => {
    vi.useFakeTimers();
    editor = new Editor({extensions: [StarterKit, ...createDiaryTableExtensions()], content: '<p>第一行</p><p>第二行</p>'});
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(Component, {view: true, panelOpen: true, editor});
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
    await nextTick(); button('表格').click();
    await vi.advanceTimersByTimeAsync(100); await nextTick();
    expect(editor.state.doc.child(0).type.name).toBe('table');
    expect(editor.state.doc.child(0).childCount).toBe(3);
    expect(button('添加行')).toBeDefined();
    expect(button('添加列')).toBeDefined();
  });
});
