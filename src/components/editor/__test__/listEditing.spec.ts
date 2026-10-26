// @vitest-environment happy-dom
import {afterEach, describe, expect, it} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import {ListIndentation, canChangeEditorListLevel, changeEditorListLevel, toggleEditorList} from '../listEditing';
import {htmlToDiaryContent, htmlToMarkdown, markdownToHtml, diaryContentToHtml} from '../markdownConverter';

let editor: Editor | undefined;
afterEach(() => { editor?.destroy(); editor = undefined; });
function createEditor(content: string) {
  editor = new Editor({
    extensions: [StarterKit, TaskList, TaskItem.configure({nested: true}), ListIndentation],
    content,
  });
  return editor;
}
function selectText(current: Editor, text: string) {
  let position = -1;
  current.state.doc.descendants((node, pos) => {
    if (node.isText && node.text === text) position = pos;
  });
  expect(position).toBeGreaterThanOrEqual(0);
  current.commands.setTextSelection(position);
}

function pressTab(current: Editor, shiftKey = false): boolean {
  let handled = false;
  const event = new KeyboardEvent('keydown', {key: 'Tab', code: 'Tab', shiftKey});
  current.view.someProp('handleKeyDown', handler => {
    handled = Boolean(handler(current.view, event));
    return handled;
  });
  return handled;
}

describe('multi-level editor lists', () => {
  it.each(['bulletList', 'orderedList'] as const)('converts selected paragraphs into %s in one operation', type => {
    const current = createEditor('<p>第一行</p><p><strong>第二行</strong></p><p>不选中</p>');
    current.commands.setTextSelection({from: 1, to: 9});
    expect(toggleEditorList(current, type)).toBe(true);
    expect(current.state.doc.child(0).type.name).toBe(type);
    expect(current.state.doc.child(0).childCount).toBe(2);
    expect(current.state.doc.child(0).child(1).textContent).toBe('第二行');
    expect(current.state.doc.child(1).textContent).toBe('不选中');
    expect(current.getHTML()).toContain('<strong>第二行</strong>');
  });

  it.each(['- 第一项\n- 第二项', '1. 第一项\n2. 第二项', '- [ ] 第一项\n- [x] 第二项'])
    ('indents with Tab, reloads without loss and outdents with Shift+Tab: %s', source => {
      const current = createEditor(markdownToHtml(source));
      selectText(current, '第二项');
      expect(pressTab(current)).toBe(true);
      expect(current.state.doc.child(0).childCount).toBe(1);
      expect(current.state.doc.child(0).child(0).child(1).child(0).textContent).toBe('第二项');
      const saved = htmlToDiaryContent(current.getHTML());
      const before = current.getJSON();
      current.commands.setContent(diaryContentToHtml(saved, {}));
      expect(current.getJSON()).toEqual(before);
      selectText(current, '第二项');
      expect(pressTab(current, true)).toBe(true);
      expect(current.state.doc.child(0).childCount).toBe(2);
    });

  it('keeps the first item in place when it cannot be indented', () => {
    const current = createEditor(markdownToHtml('- 第一项\n- 第二项'));
    selectText(current, '第一项');
    const before = current.getJSON();
    expect(pressTab(current)).toBe(true);
    expect(current.getJSON()).toEqual(before);
  });

  it('indents multiple selected items together and supports one-step undo', () => {
    const current = createEditor(markdownToHtml('- 第一项\n- 第二项\n- 第三项'));
    const positions: number[] = [];
    current.state.doc.descendants((node, pos) => { if (node.isText) positions.push(pos); });
    current.commands.setTextSelection({from: positions[1], to: positions[2] + 3});
    const before = current.getJSON();
    expect(changeEditorListLevel(current, true)).toBe(true);
    expect(current.state.doc.child(0).childCount).toBe(1);
    expect(current.state.doc.child(0).child(0).child(1).childCount).toBe(2);
    expect(current.commands.undo()).toBe(true);
    expect(current.getJSON()).toEqual(before);
  });

  it('does not intercept Tab outside lists', () => {
    const current = createEditor('<p>正文</p>');
    expect(changeEditorListLevel(current, true)).toBe(false);
    expect(pressTab(current)).toBe(false);
    expect(toggleEditorList(null, 'bulletList')).toBe(false);
  });

  it('checks available list levels without changing the document or selection', () => {
    const current = createEditor(markdownToHtml('- 第一项\n- 第二项'));
    selectText(current, '第一项');
    expect(canChangeEditorListLevel(current, true)).toBe(false);
    expect(canChangeEditorListLevel(current, false)).toBe(true);
    selectText(current, '第二项');
    const before = current.state;
    expect(canChangeEditorListLevel(current, true)).toBe(true);
    expect(canChangeEditorListLevel(current, false)).toBe(true);
    expect(current.state.doc).toBe(before.doc);
    expect(current.state.selection.eq(before.selection)).toBe(true);
    current.setEditable(false);
    expect(canChangeEditorListLevel(current, false)).toBe(false);
    expect(canChangeEditorListLevel(null, true)).toBe(false);
  });

  it('refuses to wrap non-text blocks into a Markdown list', () => {
    const current = createEditor('<p>正文</p><pre><code>代码块</code></pre>');
    current.commands.setTextSelection({from: 1, to: current.state.doc.content.size - 1});
    const before = current.getJSON();
    expect(toggleEditorList(current, 'bulletList')).toBe(false);
    expect(current.getJSON()).toEqual(before);
  });

  it.each([
    '- 父级\n  - 子级\n    1. 孙级\n- 第二项',
    '5. 第一项\n   - 子级\n6. 第二项',
    '- [ ] 父级\n  - [x] 子级\n- [x] 第二项',
    '- 第一项\n\n  第二段\n- 第二项',
  ])('round-trips nested Markdown without dropping any content: %s', source => {
    const firstHtml = markdownToHtml(source);
    const saved = htmlToMarkdown(firstHtml);
    expect(markdownToHtml(saved)).toBe(firstHtml);
  });
});
