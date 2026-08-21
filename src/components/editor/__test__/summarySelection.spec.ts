// @vitest-environment happy-dom
import {describe, expect, it} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {TextSelection} from '@tiptap/pm/state';
import {SummaryNode} from '../tiptap-extensions/SummaryNode';
import {
  appendSelectionToSummary,
  listSummaryTargets,
  readPlainTextSelection,
  replaceSelectionWithSummary,
} from '../summarySelection';

function textRanges(editor: Editor): Array<{from: number; to: number}> {
  const ranges: Array<{from: number; to: number}> = [];
  editor.state.doc.descendants((node, position) => {
    if (node.isText) ranges.push({from: position, to: position + node.nodeSize});
  });
  return ranges;
}

function selectFromFirstToLastText(editor: Editor) {
  const ranges = textRanges(editor);
  editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(
    editor.state.doc,
    ranges[0].from,
    ranges[ranges.length - 1].to,
  )));
}

describe('Summary text selection operations', () => {
  it('reads plain text across text blocks with line breaks', () => {
    const editor = new Editor({
      extensions: [StarterKit, SummaryNode],
      content: '<p>第一段</p><h2>第二段</h2>',
    });
    selectFromFirstToLastText(editor);

    expect(readPlainTextSelection(editor.state)?.text).toBe('第一段\n第二段');
    editor.destroy();
  });

  it('rejects a selection containing a structured Summary node', () => {
    const editor = new Editor({
      extensions: [StarterKit, SummaryNode],
      content: '<p>前文</p><details class="editor-summary" data-summary="摘要" data-content="内容"></details><p>后文</p>',
    });
    selectFromFirstToLastText(editor);

    expect(readPlainTextSelection(editor.state)).toBeNull();
    editor.destroy();
  });

  it('replaces selected text with a Summary node', () => {
    const editor = new Editor({
      extensions: [StarterKit, SummaryNode],
      content: '<p>保留 选中文字 尾部</p>',
    });
    const text = textRanges(editor)[0];
    const from = text.from + '保留 '.length;
    const to = from + '选中文字'.length;
    editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, from, to)));
    const source = readPlainTextSelection(editor.state)!;
    const transaction = replaceSelectionWithSummary(editor.state, source, {
      summary: '外显',
      content: source.text,
    });

    expect(transaction).not.toBeNull();
    editor.view.dispatch(transaction!);
    expect(editor.getJSON().content).toEqual([
      expect.objectContaining({type: 'paragraph'}),
      {type: 'summaryNode', attrs: {summary: '外显', content: '选中文字'}},
      expect.objectContaining({type: 'paragraph'}),
    ]);
    expect(editor.getText()).toContain('保留');
    expect(editor.getText()).toContain('尾部');
    editor.destroy();
  });

  it('moves selected text into an existing Summary node', () => {
    const editor = new Editor({
      extensions: [StarterKit, SummaryNode],
      content: '<details class="editor-summary" data-summary="已有" data-content="原内容"></details><p>移动我</p>',
    });
    const target = listSummaryTargets(editor.state.doc)[0];
    const selected = textRanges(editor)[0];
    editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(
      editor.state.doc,
      selected.from,
      selected.to,
    )));
    const source = readPlainTextSelection(editor.state)!;
    const transaction = appendSelectionToSummary(editor.state, source, target.position);

    expect(transaction).not.toBeNull();
    editor.view.dispatch(transaction!);
    expect(listSummaryTargets(editor.state.doc)[0].content).toBe('原内容\n移动我');
    expect(editor.getText()).not.toContain('移动我');
    editor.destroy();
  });

  it('keeps the target update when the Summary follows the selected text', () => {
    const editor = new Editor({
      extensions: [StarterKit, SummaryNode],
      content: '<p>前面的文字</p><details class="editor-summary" data-summary="后面的折叠" data-content=""></details>',
    });
    const selected = textRanges(editor)[0];
    editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(
      editor.state.doc,
      selected.from,
      selected.to,
    )));
    const source = readPlainTextSelection(editor.state)!;
    const target = listSummaryTargets(editor.state.doc)[0];
    const transaction = appendSelectionToSummary(editor.state, source, target.position);

    expect(transaction).not.toBeNull();
    editor.view.dispatch(transaction!);
    expect(listSummaryTargets(editor.state.doc)[0]).toEqual(expect.objectContaining({
      summary: '后面的折叠',
      content: '前面的文字',
    }));
    editor.destroy();
  });
});
