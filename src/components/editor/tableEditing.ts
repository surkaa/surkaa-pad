import {Table, TableCell, TableHeader, TableRow} from '@tiptap/extension-table';
import {DOMParser as ProseMirrorDOMParser} from '@tiptap/pm/model';
import type {EditorView} from '@tiptap/pm/view';
import {renderPastedMarkdownTable} from './markdownParser';

export function createDiaryTableExtensions() {
  return [
    Table.configure({renderWrapper: true, resizable: false, HTMLAttributes: {class: 'editor-table'}}),
    TableRow,
    // 第一版只保存 Markdown 表格：单元格内不允许嵌入附件、子表格等结构化块。
    TableHeader.extend({content: 'paragraph+'}),
    TableCell.extend({content: 'paragraph+'}),
  ];
}

export function pasteMarkdownTable(view: EditorView, event: ClipboardEvent): boolean {
  if (!view.editable || !event.clipboardData) return false;
  const {selection} = view.state;
  for (let depth = selection.$from.depth; depth > 0; depth--) {
    const node = selection.$from.node(depth);
    if (node.type.name === 'table' || node.type.spec.code) return false;
  }
  const html = renderPastedMarkdownTable(event.clipboardData.getData('text/plain'));
  if (!html) return false;
  const container = document.createElement('div');
  container.innerHTML = html;
  const slice = ProseMirrorDOMParser.fromSchema(view.state.schema).parseSlice(container);
  event.preventDefault();
  view.dispatch(view.state.tr.replaceSelection(slice).scrollIntoView());
  return true;
}
