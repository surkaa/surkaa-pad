import {Table, TableCell, TableHeader, TableRow} from '@tiptap/extension-table';
import {Extension} from '@tiptap/vue-3';
import {Plugin} from '@tiptap/pm/state';
import {normalizeTableHeaders} from './tableCommands';

const MarkdownTableHeaders = Extension.create({
  name: 'markdownTableHeaders',
  addProseMirrorPlugins() {
    return [new Plugin({
      appendTransaction(transactions, _oldState, state) {
        if (!transactions.some(transaction => transaction.docChanged)) return null;
        const transaction = normalizeTableHeaders(state.tr);
        return transaction.docChanged ? transaction : null;
      },
    })];
  },
});

export function createDiaryTableExtensions() {
  return [
    Table.configure({renderWrapper: true, resizable: false, HTMLAttributes: {class: 'editor-table'}}),
    TableRow,
    // 第一版只保存 Markdown 表格：单元格内不允许嵌入附件、子表格等结构化块。
    TableHeader.extend({content: 'paragraph+'}),
    TableCell.extend({content: 'paragraph+'}),
    MarkdownTableHeaders,
  ];
}
