// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {createDiaryTableExtensions} from '../tableEditing';
import {pasteEditorMarkdown} from '../markdownPaste';
import {diaryContentToHtml, htmlToDiaryContent, htmlToMarkdown, markdownToHtml} from '../markdownConverter';
import {ListIndentation} from '../listEditing';
import {renderPastedEditorMarkdown} from '../markdownParser';

let editor: Editor | undefined;
afterEach(() => { editor?.destroy(); editor = undefined; document.body.innerHTML = ''; });
function createEditor(content = '<p></p>') {
  const element = document.createElement('div');
  document.body.append(element);
  editor = new Editor({element, extensions: [StarterKit, ListIndentation, ...createDiaryTableExtensions()], content});
  return editor;
}
function clipboardEvent(text: string) {
  return {clipboardData: {getData: (kind: string) => kind === 'text/plain' ? text : ''}, preventDefault: vi.fn()} as unknown as ClipboardEvent;
}
function pressTab(current: Editor) {
  const event = new KeyboardEvent('keydown', {key: 'Tab', code: 'Tab'});
  return current.view.someProp('handleKeyDown', handler => handler(current.view, event));
}

describe('Markdown tables', () => {
  it.each([
    '| 名称 | 数量 |\n| --- | --- |\n| 苹果 | 3 |',
    '名称 | 数量\n:--- | ---:\n苹果 | 3',
    '| 左 | 中 | 右 |\n| :--- | :---: | ---: |\n| **粗体** | _斜体_ | [链接](https://example.com) |',
    '| A | B |\n| --- | --- |\n| a\\|b | `x\\|y` |',
    '| A | B |\n| --- | --- |\n| 第一行<br>第二行 | <u>下划线</u> |',
    '|  |  |\n| --- | --- |\n|  |  |',
    '表格前\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n表格后',
  ])('loads, saves and reloads without losing structure or formatting: %s', source => {
    const html = markdownToHtml(source);
    const current = createEditor(html + (html.endsWith('</table>') ? '<p></p>' : ''));
    const before = current.getJSON();
    expect(current.view.dom.querySelector('.tableWrapper .editor-table')).not.toBeNull();
    const saved = htmlToDiaryContent(current.getHTML());
    expect(saved.nodes.every(node => node.type === 'markdown')).toBe(true);
    expect(saved.nodes[0]).toMatchObject({type: 'markdown', text: expect.stringContaining('|')});
    current.commands.setContent(diaryContentToHtml(saved, {}));
    expect(current.getJSON()).toEqual(before);
  });

  it('preserves literal Markdown/HTML characters and backslashes inside cells', () => {
    const current = createEditor('<table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr>'
      + '<td><p>literal *bold* &amp; &lt;script&gt; C:\\a|b [link]</p></td>'
      + '<td><p><code>C:\\a|b `ticks`</code></p></td></tr></tbody></table><p></p>');
    const before = current.getJSON();
    const saved = htmlToDiaryContent(current.getHTML());
    current.commands.setContent(diaryContentToHtml(saved, {}));
    expect(current.getJSON()).toEqual(before);
  });

  it('exports row/column spans as a rectangular Markdown table without dropping text', () => {
    const source = htmlToMarkdown('<table><tr><th colspan="2">标题</th></tr>'
      + '<tr><td rowspan="2">跨行</td><td>第一行</td></tr><tr><td>第二行</td></tr></table>');
    expect(source).toBe('| 标题 |  |\n| --- | --- |\n| 跨行 | 第一行 |\n|  | 第二行 |');
  });

  it('keeps Tab navigation inside the table and appends a row at the last cell', () => {
    const current = createEditor(markdownToHtml('| A | B |\n| --- | --- |\n| 1 | 2 |') + '<p></p>');
    const cells: number[] = [];
    current.state.doc.descendants((node, pos) => {
      if (node.type.name === 'tableCell' || node.type.name === 'tableHeader') cells.push(pos + 2);
    });
    current.commands.setTextSelection(cells[0]);
    expect(pressTab(current)).toBe(true);
    expect(current.state.selection.from).toBe(cells[1]);
    current.commands.setTextSelection(cells[cells.length - 1]);
    expect(pressTab(current)).toBe(true);
    expect(current.state.doc.child(0).childCount).toBe(3);
  });

  it('does not allow attachments or nested tables in a cell', () => {
    const current = createEditor();
    expect(current.schema.nodes.tableCell.spec.content).toBe('paragraph+');
    expect(current.schema.nodes.tableHeader.spec.content).toBe('paragraph+');
  });
});

describe('pasting Markdown tables', () => {
  it('replaces a selection in a single undoable operation', () => {
    const current = createEditor('<p>前面</p><p>替换</p><p>后面</p>');
    current.commands.setTextSelection({from: 5, to: 7});
    const before = current.getJSON();
    const event = clipboardEvent('| A | B |\n| --- | --- |\n| 1 | 2 |');
    expect(pasteEditorMarkdown(current.view, event)).toBe(true);
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(current.view.dom.querySelectorAll('table')).toHaveLength(1);
    expect(current.state.doc.textContent).toContain('前面');
    expect(current.state.doc.textContent).toContain('后面');
    expect(current.commands.undo()).toBe(true);
    expect(current.getJSON()).toEqual(before);
  });

  it('escapes pasted HTML and does not create executable links or remote images', () => {
    const current = createEditor();
    const event = clipboardEvent('| A | B |\n| --- | --- |\n| <script>alert(1)</script> | ![图片](https://example.com/a.jpg) |');
    expect(pasteEditorMarkdown(current.view, event)).toBe(true);
    expect(current.view.dom.querySelector('script, img')).toBeNull();
    expect(current.state.doc.textContent).toContain('<script>alert(1)</script>');
    const badLink = renderPastedEditorMarkdown('| A |\n| --- |\n| [bad](javascript:alert%281%29) |');
    expect(badLink).not.toContain('href="javascript:');
  });

  it.each(['普通文本', 'a | b', '```\n| A | B |\n| --- | --- |\n```'])
    ('leaves ordinary text and code samples to the default paste handler: %s', text => {
      const current = createEditor();
      const before = current.getJSON();
      const event = clipboardEvent(text);
      expect(pasteEditorMarkdown(current.view, event)).toBe(false);
      expect(event.preventDefault).not.toHaveBeenCalled();
      expect(current.getJSON()).toEqual(before);
    });

  it('does not convert tables inside a code block or an existing table', () => {
    const current = createEditor('<pre><code>示例</code></pre><p></p>');
    const text = '| A | B |\n| --- | --- |\n| 1 | 2 |';
    expect(pasteEditorMarkdown(current.view, clipboardEvent(text))).toBe(false);
    current.commands.setContent(markdownToHtml(text) + '<p></p>');
    current.commands.setTextSelection(4);
    expect(pasteEditorMarkdown(current.view, clipboardEvent(text))).toBe(false);
    current.setEditable(false);
    expect(pasteEditorMarkdown(current.view, clipboardEvent(text))).toBe(false);
  });
});
