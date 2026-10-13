import {Table, TableCell, TableHeader, TableRow} from '@tiptap/extension-table';

export function createDiaryTableExtensions() {
  return [
    Table.configure({renderWrapper: true, resizable: false, HTMLAttributes: {class: 'editor-table'}}),
    TableRow,
    // 第一版只保存 Markdown 表格：单元格内不允许嵌入附件、子表格等结构化块。
    TableHeader.extend({content: 'paragraph+'}),
    TableCell.extend({content: 'paragraph+'}),
  ];
}
