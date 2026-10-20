// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {createDiaryTableExtensions} from '../tableEditing';
import {
  canRunTableAction, runTableAction, tableTargetFromElement, validTableSize, type TableAction,
} from '../tableCommands';
import {diaryContentToHtml, htmlToDiaryContent} from '../markdownConverter';

let editor: Editor;
afterEach(() => { editor?.destroy(); document.body.innerHTML = ''; });
function createTable(rows = 3, columns = 3) {
  const element = document.createElement('div'); document.body.append(element);
  editor = new Editor({element, extensions: [StarterKit, ...createDiaryTableExtensions()],
    content: '<p>之前</p><table>' + Array.from({length: rows}, (_, row) => '<tr>'
      + Array.from({length: columns}, (_, column) => {
        const tag = row === 0 ? 'th' : 'td';
        return `<${tag}><p><strong>${row}${column}</strong></p></${tag}>`;
      }).join('') + '</tr>').join('') + '</table><p>之后</p>'});
  return editor;
}
function target(row: number, column: number) {
  const cell = editor.view.dom.querySelectorAll('tr')[row].children[column];
  return tableTargetFromElement(editor, cell)!;
}
function contents() {
  return Array.from(editor.view.dom.querySelectorAll('tr'), row =>
    Array.from(row.children, cell => cell.textContent));
}

describe('table commands', () => {
  it.each([
    [3, 3, true], [1, 1, true], [100, 10, true], [100, 20, false],
    [0, 3, false], [3, 0, false], [101, 1, false], [1, 21, false],
    [NaN, 1, false], [1.5, 3, false], [Infinity, 1, false],
  ])('validates dimensions %s × %s', (rows, columns, valid) => {
    expect(validTableSize(rows, columns)).toBe(valid);
  });

  it.each<[TableAction, number, number, string[][]]>([
    ['rowUp', 1, 1, [['10', '11', '12'], ['00', '01', '02'], ['20', '21', '22']]],
    ['rowDown', 0, 1, [['10', '11', '12'], ['00', '01', '02'], ['20', '21', '22']]],
    ['columnLeft', 1, 1, [['01', '00', '02'], ['11', '10', '12'], ['21', '20', '22']]],
    ['columnRight', 1, 1, [['00', '02', '01'], ['10', '12', '11'], ['20', '22', '21']]],
    ['rowBefore', 0, 1, [['', '', ''], ['00', '01', '02'], ['10', '11', '12'], ['20', '21', '22']]],
    ['rowAfter', 2, 1, [['00', '01', '02'], ['10', '11', '12'], ['20', '21', '22'], ['', '', '']]],
    ['columnBefore', 1, 0, [['', '00', '01', '02'], ['', '10', '11', '12'], ['', '20', '21', '22']]],
    ['columnAfter', 1, 2, [['00', '01', '02', ''], ['10', '11', '12', ''], ['20', '21', '22', '']]],
    ['deleteRow', 0, 1, [['10', '11', '12'], ['20', '21', '22']]],
    ['deleteColumn', 1, 1, [['00', '02'], ['10', '12'], ['20', '22']]],
    ['clearRow', 1, 1, [['00', '01', '02'], ['', '', ''], ['20', '21', '22']]],
    ['clearColumn', 1, 1, [['00', '', '02'], ['10', '', '12'], ['20', '', '22']]],
    ['deleteTable', 1, 1, []],
  ])('%s targets the clicked cell, survives saving, and is a single undo/redo', (action, row, column, expected) => {
    createTable();
    editor.commands.setTextSelection(1); // 光标故意留在表格外。
    const before = editor.getJSON();
    const focus = vi.spyOn(editor.view, 'focus');
    expect(runTableAction(editor, target(row, column), action)).toBe(true);
    expect(focus).not.toHaveBeenCalled();
    expect(contents()).toEqual(expected);
    expect(editor.state.doc.textContent).toContain('之前');
    expect(editor.state.doc.textContent).toContain('之后');
    const after = editor.getJSON();
    expect(editor.commands.undo()).toBe(true);
    expect(editor.getJSON()).toEqual(before);
    expect(editor.commands.redo()).toBe(true);
    expect(editor.getJSON()).toEqual(after);
    editor.commands.setContent(diaryContentToHtml(htmlToDiaryContent(editor.getHTML()), {}));
    expect(editor.getJSON()).toEqual(after);
  });

  it('blocks moves across boundaries and deletion of the last row/column', () => {
    createTable(1, 1);
    for (const action of ['rowUp', 'rowDown', 'columnLeft', 'columnRight', 'deleteRow', 'deleteColumn'] as const) {
      expect(canRunTableAction(editor, target(0, 0), action)).toBe(false);
      expect(runTableAction(editor, target(0, 0), action)).toBe(false);
    }
    expect(runTableAction(editor, target(0, 0), 'deleteTable')).toBe(true);
  });

  it('rejects stale targets, readonly editors and unrelated DOM', () => {
    createTable();
    expect(tableTargetFromElement(editor, document.body)).toBeNull();
    const stale = target(1, 1);
    editor.commands.insertContent('改变');
    const before = editor.getJSON();
    expect(runTableAction(editor, stale, 'deleteTable')).toBe(false);
    const current = target(1, 1);
    editor.setEditable(false);
    expect(runTableAction(editor, current, 'deleteTable')).toBe(false);
    expect(editor.getJSON()).toEqual(before);
  });

  it('keeps two consecutive menu operations separately undoable', () => {
    createTable();
    const before = editor.getJSON();
    runTableAction(editor, target(1, 1), 'rowAfter');
    const intermediate = editor.getJSON();
    runTableAction(editor, target(1, 1), 'columnAfter');
    editor.commands.undo(); expect(editor.getJSON()).toEqual(intermediate);
    editor.commands.undo(); expect(editor.getJSON()).toEqual(before);
  });
});
