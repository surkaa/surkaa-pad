import type {Editor} from '@tiptap/vue-3';
import type {Node} from '@tiptap/pm/model';
import {TextSelection, type Command, type Transaction} from '@tiptap/pm/state';
import {closeHistory} from '@tiptap/pm/history';
import {
  addColumnAfter, addColumnBefore, addRowAfter, addRowBefore,
  deleteColumn, deleteRow, deleteTable, findTable, moveTableColumn, moveTableRow, TableMap,
} from '@tiptap/pm/tables';

export const MAX_TABLE_ROWS = 100;
export const MAX_TABLE_COLUMNS = 20;
export const MAX_TABLE_CELLS = 1000;

export function validTableSize(rows: number, columns: number): boolean {
  return Number.isInteger(rows) && Number.isInteger(columns)
    && rows >= 1 && rows <= MAX_TABLE_ROWS && columns >= 1 && columns <= MAX_TABLE_COLUMNS
    && rows * columns <= MAX_TABLE_CELLS;
}

export interface TableTarget {
  // 菜单打开期间内容若被外部更新，不可再按旧位置修改另一张表。
  document: Node;
  cellPosition: number;
}

export const TABLE_ACTIONS = [
  ['rowBefore', '在上方插入行'], ['rowAfter', '在下方插入行'],
  ['columnBefore', '在左侧插入列'], ['columnAfter', '在右侧插入列'],
  ['rowUp', '当前行上移'], ['rowDown', '当前行下移'],
  ['columnLeft', '当前列左移'], ['columnRight', '当前列右移'],
  ['clearRow', '清空当前行'], ['clearColumn', '清空当前列'],
  ['deleteRow', '删除当前行'], ['deleteColumn', '删除当前列'], ['deleteTable', '删除整个表格'],
] as const;
export type TableAction = typeof TABLE_ACTIONS[number][0];

export function tableTargetAt(editor: Editor, position: number): TableTarget | null {
  if (position < 0 || position > editor.state.doc.content.size) return null;
  const resolved = editor.state.doc.resolve(position);
  for (let depth = resolved.depth; depth > 0; depth--) {
    if (['tableCell', 'tableHeader'].includes(resolved.node(depth).type.name)) {
      return {document: editor.state.doc, cellPosition: resolved.before(depth)};
    }
  }
  return null;
}

export function tableTargetFromElement(editor: Editor, target: EventTarget | null): TableTarget | null {
  const cell = target instanceof Element ? target.closest('td, th') : null;
  if (!cell || !editor.view.dom.contains(cell)) return null;
  return tableTargetAt(editor, editor.view.posAtDOM(cell, 0));
}

function tableContext(editor: Editor, target: TableTarget) {
  if (editor.isDestroyed || !editor.isEditable || target.document !== editor.state.doc) return null;
  const resolved = editor.state.doc.resolve(target.cellPosition + 1);
  const table = findTable(resolved);
  if (!table) return null;
  const map = TableMap.get(table.node);
  // 第一版只操作普通矩形表格；不把跨行/列单元格误当成单个格子移动。
  if (new Set(map.map).size !== map.width * map.height || map.problems?.length) return null;
  const rect = map.findCell(target.cellPosition - table.start);
  return {table, map, row: rect.top, column: rect.left};
}

export function canRunTableAction(editor: Editor, target: TableTarget, action: TableAction): boolean {
  const context = tableContext(editor, target);
  if (!context) return false;
  const {row, column, map} = context;
  switch (action) {
    case 'rowBefore': case 'rowAfter': return validTableSize(map.height + 1, map.width);
    case 'columnBefore': case 'columnAfter': return validTableSize(map.height, map.width + 1);
    case 'rowUp': return row > 0;
    case 'rowDown': return row < map.height - 1;
    case 'columnLeft': return column > 0;
    case 'columnRight': return column < map.width - 1;
    case 'deleteRow': return map.height > 1;
    case 'deleteColumn': return map.width > 1;
    default: return true;
  }
}

/** Markdown 的第一行固定是表头；移动/增删首行也要在同一次撤销操作中同步节点类型。 */
export function normalizeTableHeaders(transaction: Transaction): Transaction {
  const {schema} = transaction.doc.type;
  transaction.doc.descendants((node, position) => {
    if (node.type.name !== 'table') return;
    let rowPosition = position + 1;
    node.forEach((row, _offset, rowIndex) => {
      let cellPosition = rowPosition + 1;
      row.forEach(cell => {
        const type = rowIndex === 0 ? schema.nodes.tableHeader : schema.nodes.tableCell;
        if (cell.type !== type) transaction.setNodeMarkup(cellPosition, type, cell.attrs, cell.marks);
        cellPosition += cell.nodeSize;
      });
      rowPosition += row.nodeSize;
    });
    return false;
  });
  return transaction;
}

export function runTableAction(editor: Editor, target: TableTarget, action: TableAction): boolean {
  if (!canRunTableAction(editor, target, action)) return false;
  const {table, map, row, column} = tableContext(editor, target)!;
  // 不依赖打开菜单前的光标位置，也不调用 focus()，避免移动端弹出软键盘。
  const state = editor.state.apply(editor.state.tr.setSelection(
    TextSelection.near(editor.state.doc.resolve(target.cellPosition + 2)),
  ));
  let command: Command;
  switch (action) {
    case 'rowBefore': command = addRowBefore; break;
    case 'rowAfter': command = addRowAfter; break;
    case 'columnBefore': command = addColumnBefore; break;
    case 'columnAfter': command = addColumnAfter; break;
    case 'deleteRow': command = deleteRow; break;
    case 'deleteColumn': command = deleteColumn; break;
    case 'deleteTable': command = deleteTable; break;
    case 'rowUp': case 'rowDown':
      command = moveTableRow({from: row, to: row + (action === 'rowUp' ? -1 : 1), select: false}); break;
    case 'columnLeft': case 'columnRight':
      command = moveTableColumn({from: column, to: column + (action === 'columnLeft' ? -1 : 1), select: false}); break;
    case 'clearRow': case 'clearColumn': {
      const positions = map.map.filter((_position, index) => action === 'clearRow'
        ? Math.floor(index / map.width) === row : index % map.width === column);
      const transaction = state.tr;
      for (const position of positions.sort((a, b) => b - a)) {
        const start = table.start + position;
        const cell = state.doc.nodeAt(start)!;
        transaction.replaceWith(start + 1, start + cell.nodeSize - 1, state.schema.nodes.paragraph.create());
      }
      editor.view.dispatch(closeHistory(transaction));
      return true;
    }
  }
  return command(state, transaction => editor.view.dispatch(closeHistory(normalizeTableHeaders(transaction))));
}
