// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import {DiaryDocument} from '../editorSchema';
import {ImageNode} from '../tiptap-extensions';
import {createDiaryTableExtensions} from '../tableEditing';
import {pasteEditorMarkdown} from '../markdownPaste';
import {diaryContentToHtml, htmlToDiaryContent} from '../markdownConverter';
import type {Node as ProseMirrorNode} from '@tiptap/pm/model';

let editor: Editor | undefined;
afterEach(() => { editor?.destroy(); editor = undefined; });

function createEditor(content = '<p></p>') {
  editor = new Editor({
    extensions: [StarterKit.configure({document: false}), DiaryDocument,
      TaskList, TaskItem.configure({nested: true}), ...createDiaryTableExtensions(),
      ImageNode.extend({group: 'diaryBlock'})],
    content,
    editorProps: {handlePaste: pasteEditorMarkdown},
  });
  return editor;
}

function clipboardEvent(text: string, html = '') {
  return {
    clipboardData: {getData: (kind: string) => kind === 'text/plain' ? text : kind === 'text/html' ? html : '', files: []},
    preventDefault: vi.fn(),
  } as unknown as ClipboardEvent;
}

function listItems(doc: ProseMirrorNode) {
  const items: Array<{text: string; level: number}> = [];
  function visit(node: ProseMirrorNode, level: number) {
    if (['bulletList', 'orderedList', 'taskList'].includes(node.type.name)) level++;
    if (['listItem', 'taskItem'].includes(node.type.name)) {
      items.push({text: node.child(0).textContent, level});
    }
    node.forEach(child => visit(child, level));
  }
  visit(doc, 0);
  return items;
}

const source = 'list test\n\n- 1\n- 2\n  - 3\n    - 4\n- 5';
const expectedItems = [
  {text: '1', level: 1}, {text: '2', level: 1},
  {text: '3', level: 2}, {text: '4', level: 3}, {text: '5', level: 1},
];

describe('pasting Markdown lists', () => {
  it.each(['\n', '\r\n'])('keeps all three list levels after paste, save and reopen (newline=%j)', newline => {
    const current = createEditor();
    // 浏览器从代码块复制时可能同时给出 HTML；应按原始 Markdown 缩进转换。
    const text = source.replace(/\n/g, newline);
    expect(current.view.pasteText(text, clipboardEvent(text, '<pre>list test</pre>'))).toBe(true);
    expect(listItems(current.state.doc)).toEqual(expectedItems);
    expect(current.state.doc.child(0).textContent).toBe('list test');
    const before = htmlToDiaryContent(current.getHTML());
    const beforeList = current.state.doc.child(1).toJSON();
    for (let cycle = 0; cycle < 3; cycle++) {
      const saved = JSON.parse(JSON.stringify(htmlToDiaryContent(editor!.getHTML())));
      editor!.destroy();
      const reopened = createEditor(diaryContentToHtml(saved, {}));
      expect(listItems(reopened.state.doc)).toEqual(expectedItems);
      expect(reopened.state.doc.child(1).toJSON()).toEqual(beforeList);
      expect(htmlToDiaryContent(reopened.getHTML())).toEqual(before);
    }
  });

  it.each([
    ['1. 父级\n   1. 子级\n      1. 孙级\n2. 末项', 'orderedList'],
    ['- [ ] 父级\n  - [x] 子级\n    - [ ] 孙级\n- [x] 末项', 'taskList'],
  ])('also preserves nested %s', (text, type) => {
    const current = createEditor();
    expect(pasteEditorMarkdown(current.view, clipboardEvent(text))).toBe(true);
    expect(current.state.doc.child(0).type.name).toBe(type);
    expect(listItems(current.state.doc)).toEqual([
      {text: '父级', level: 1}, {text: '子级', level: 2},
      {text: '孙级', level: 3}, {text: '末项', level: 1},
    ]);
    const before = current.getJSON();
    current.commands.setContent(diaryContentToHtml(htmlToDiaryContent(current.getHTML()), {}));
    expect(current.getJSON()).toEqual(before);
  });

  it('replaces selected text without dropping the surrounding paragraphs and can undo once', () => {
    const current = createEditor('<p>前面</p><p>替换</p><p>后面</p>');
    current.commands.setTextSelection({from: 5, to: 7});
    const before = current.getJSON();
    expect(pasteEditorMarkdown(current.view, clipboardEvent(source))).toBe(true);
    expect(listItems(current.state.doc)).toEqual(expectedItems);
    expect(current.state.doc.textContent).toContain('前面');
    expect(current.state.doc.textContent).toContain('后面');
    expect(current.commands.undo()).toBe(true);
    expect(current.getJSON()).toEqual(before);
  });

  it('does not execute pasted HTML, javascript links or remote image requests', () => {
    const current = createEditor();
    const text = '- <script>alert(1)</script>\n  - ![图片](https://example.com/a.jpg)\n- [链接](javascript:alert%281%29)';
    expect(pasteEditorMarkdown(current.view, clipboardEvent(text))).toBe(true);
    expect(current.view.dom.querySelector('script, img, a[href^="javascript:"]')).toBeNull();
    expect(current.state.doc.textContent).toContain('<script>alert(1)</script>');
    expect(current.state.doc.textContent).toContain('[图片：图片]');
  });

  it.each(['普通文本\n下一行', '```markdown\n- 父级\n  - 子级\n```', '`- 示例`'])
    ('leaves ordinary text and fenced code samples to the default paste handler: %s', text => {
      const current = createEditor();
      const event = clipboardEvent(text);
      const before = current.getJSON();
      expect(pasteEditorMarkdown(current.view, event)).toBe(false);
      expect(event.preventDefault).not.toHaveBeenCalled();
      expect(current.getJSON()).toEqual(before);
    });

  it('does not convert lists inside code blocks or table cells, or when read-only', () => {
    const current = createEditor('<pre><code>代码</code></pre><p></p>');
    expect(pasteEditorMarkdown(current.view, clipboardEvent(source))).toBe(false);
    current.commands.setContent('<table><tr><th><p>单元格</p></th></tr></table><p></p>');
    current.commands.setTextSelection(4);
    expect(pasteEditorMarkdown(current.view, clipboardEvent(source))).toBe(false);
    current.commands.setContent('<p></p>');
    current.setEditable(false);
    expect(pasteEditorMarkdown(current.view, clipboardEvent(source))).toBe(false);
  });
});
