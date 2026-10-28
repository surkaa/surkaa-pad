// @vitest-environment happy-dom
import {afterEach, describe, expect, it} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {SummaryNode, summaryRange} from '../tiptap-extensions/SummaryNode';
import {DiaryDocument} from '../editorSchema';
import {diaryContentToHtml, htmlToDiaryContent} from '../markdownConverter';
import {FileNode} from '../tiptap-extensions/FileNode';
import {createDiaryTableExtensions} from '../tableEditing';
import type {DiaryContent} from '../../../bindings';

let editor: Editor;
function mount(content: string | DiaryContent = '<p>正文</p>') {
  const element = document.createElement('div'); document.body.append(element);
  editor = new Editor({element, extensions: [
    StarterKit.configure({document: false}), DiaryDocument.configure({summary: true}), SummaryNode,
    FileNode.extend({group: 'diaryBlock'}), ...createDiaryTableExtensions(),
  ], content: typeof content === 'string' ? content : diaryContentToHtml(content, {})});
  return element;
}
afterEach(() => {editor?.destroy(); document.body.innerHTML = '';});
function position(type: string) {
  let found = -1; editor.state.doc.descendants((node, pos) => {if (found < 0 && node.type.name === type) found = pos;});
  return found;
}
function key(key: string) {
  const event = new KeyboardEvent('keydown', {key, bubbles: true, cancelable: true});
  editor.view.dom.dispatchEvent(event); return event;
}

describe('inline editable Summary', () => {
  it.each([false, true])('moves selected content to an existing fold before/after it and undoes atomically (%s)', before => {
    const fold = {type: 'summary' as const, summary: '目标', content: [{type: 'markdown' as const, text: '已有'}]};
    const text = {type: 'markdown' as const, text: '**移入**'};
    mount({nodes: before ? [fold, text] : [text, fold]});
    let range = {from: 0, to: 0};
    editor.state.doc.descendants((node, pos) => {if (node.text === '移入') range = {from: pos, to: pos + 2};});
    editor.commands.setTextSelection(range);
    const snapshot = editor.getJSON();
    expect(editor.commands.appendToSummary(position('summaryNode'))).toBe(true);
    const saved = htmlToDiaryContent(editor.getHTML());
    expect(JSON.stringify(saved)).toContain('**移入**');
    expect(saved.nodes.filter(node => node.type === 'summary')).toHaveLength(1);
    expect(editor.commands.undo()).toBe(true); expect(editor.getJSON()).toEqual(snapshot);
  });

  it('keeps raw HTML inert while allowing the editor underline format', () => {
    const el = mount({nodes: [{type: 'summary', summary: '标题', content: [{type: 'markdown',
      text: '<script>alert(1)</script>\n\n<img src="https://example.com/leak" onerror="alert(1)">\n\n<u>下划线</u>',
    }]}]});
    expect(el.querySelector('script')).toBeNull(); expect(el.querySelector('img')).toBeNull();
    expect(el.textContent).toContain('<script>'); expect(el.querySelector('u')?.textContent).toBe('下划线');
  });

  it('enters a writable paragraph before a first attachment and keeps attachment insertion inside the fold', () => {
    mount({nodes:[{type:'summary', summary:'文件', content:[{type:'file',attachmentId:'a'}]}]});
    editor.commands.setTextSelection(position('summaryTitle') + 1); key('Enter');
    expect(editor.state.selection.$from.parent.type.name).toBe('paragraph');
    expect(summaryRange(editor.state)).not.toBeNull();
    editor.commands.insertFile({id:'b', filename:'b.txt'});
    const saved = htmlToDiaryContent(editor.getHTML());
    expect(saved.nodes[0]).toMatchObject({type:'summary', content: expect.arrayContaining([
      {type:'file',attachmentId:'a'}, {type:'file',attachmentId:'b'},
    ])});
  });

  it('wraps a mixed selection without losing formatting, files or table cells and supports undo', () => {
    mount('<p><strong>加粗</strong></p><div class="editor-file-attachment" data-id="file"></div>'
      + '<table><tbody><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></tbody></table>');
    editor.commands.selectAll(); const before = editor.getJSON();
    expect(editor.commands.insertSummary()).toBe(true);
    expect(position('summaryTitle')).toBeGreaterThanOrEqual(0);
    expect(position('fileNode')).toBeGreaterThan(0); expect(position('table')).toBeGreaterThan(0);
    expect(editor.getHTML()).toContain('<strong>加粗</strong>');
    expect(editor.commands.undo()).toBe(true); expect(editor.getJSON()).toEqual(before);
  });

  it('edits title/body in the same editor and serializes structured children through save/reopen', () => {
    mount();
    editor.commands.selectAll(); editor.commands.insertSummary();
    editor.commands.insertContent('新标题'); key('Enter'); editor.commands.insertContent('内部');
    const saved = htmlToDiaryContent(editor.getHTML());
    expect(saved.nodes[0]).toMatchObject({type: 'summary', summary: '新标题'});
    expect(JSON.stringify(saved)).toContain('内部');
    editor.commands.setContent(diaryContentToHtml(saved, {}));
    expect(htmlToDiaryContent(editor.getHTML())).toEqual(saved);
    expect(editor.view.dom.querySelector('.editor-summary-title')?.closest('[contenteditable="false"]')).toBeNull();
  });

  it('exits at the last empty paragraph or by button and can unwrap without deleting children', () => {
    const el = mount({nodes:[{type:'summary', summary:'标题', content:[{type:'markdown',text:'内部'}]}]});
    editor.commands.setTextSelection(position('summaryBody') + 2);
    editor.commands.exitSummary(); editor.commands.insertContent('外面');
    expect(htmlToDiaryContent(editor.getHTML()).nodes[1]).toMatchObject({type:'markdown',text:'外面'});
    editor.commands.setTextSelection(position('summaryTitle') + 1);
    expect(editor.commands.unwrapSummary()).toBe(true);
    expect(position('summaryNode')).toBe(-1);
    expect(editor.getText()).toContain('内部'); expect(editor.getText()).toContain('标题');
    expect(el.querySelector('.editor-summary-edit')).toBeNull();
  });

  it('keeps collapse state local and does not focus or save the document when toggled', () => {
    const el = mount({nodes:[{type:'summary', summary:'标题', content:[{type:'markdown',text:'内容'}]}]});
    const before = editor.getJSON(); const toggle = el.querySelector<HTMLButtonElement>('.editor-summary-toggle')!;
    toggle.click(); expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(editor.getJSON()).toEqual(before); expect(editor.isFocused).toBe(false);
    toggle.click(); expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(editor.getJSON()).toEqual(before);
  });

  it('rejects nested creation and protects Backspace at the start of the title', () => {
    mount(); editor.commands.insertSummary();
    expect(summaryRange(editor.state)).not.toBeNull();
    const before = editor.getJSON();
    expect(editor.commands.insertSummary()).toBe(false);
    key('Backspace'); expect(editor.getJSON()).toEqual(before);
    expect(editor.schema.nodes.summaryBody.validContent(editor.state.doc.content)).toBe(false);
  });

  it('the last empty paragraph Enter leaves the fold but ordinary paragraph Enter stays inside', () => {
    mount('<p></p>'); editor.commands.insertSummary(); key('Enter');
    expect(editor.state.selection.$from.parent.type.name).toBe('paragraph');
    key('Enter'); expect(summaryRange(editor.state)).toBeNull();
    editor.commands.insertContent('外面');
    expect(htmlToDiaryContent(editor.getHTML()).nodes[1]).toMatchObject({type:'markdown',text:'外面'});
  });
});
