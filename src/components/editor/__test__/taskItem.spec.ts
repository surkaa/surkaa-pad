// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import TaskList from '@tiptap/extension-task-list';
import {createTaskItemExtension} from '../taskItem';
import {TaskCompletionOrder} from '../taskCompletionOrder';

let editor: Editor | undefined;
afterEach(() => {
  editor?.destroy();
  editor = undefined;
  document.body.innerHTML = '';
  vi.useRealTimers();
});

function mountEditor(editable = true, readOnlyChecked?: () => boolean) {
  const element = document.createElement('div');
  document.body.append(element);
  editor = new Editor({
    element, editable,
    extensions: [StarterKit, TaskList, createTaskItemExtension(true).configure({
      HTMLAttributes: {class: 'editor-task-item'},
      onReadOnlyChecked: readOnlyChecked,
      a11y: {checkboxLabel: (node, checked) => `${checked ? '取消完成' : '完成'}：${node.textContent}`},
    }), TaskCompletionOrder],
    content: '<p>开头</p><ul data-type="taskList">'
      + '<li data-type="taskItem" data-checked="false"><p>第一项</p></li>'
      + '<li data-type="taskItem" data-checked="false"><p>第二项</p></li>'
      + '</ul><p>末尾</p>',
  });
  return {editor, element};
}

describe('mobile task checkbox focus', () => {
  it('checks and reorders without focusing the editor or requesting a scroll', async () => {
    vi.useFakeTimers();
    const {editor, element} = mountEditor();
    const focus = vi.spyOn(editor.view, 'focus');
    const transactions: boolean[] = [];
    editor.on('transaction', ({transaction}) => transactions.push(transaction.scrolledIntoView));
    const checkbox = element.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event('change', {bubbles: true}));
    await vi.advanceTimersByTimeAsync(100);

    expect(focus).not.toHaveBeenCalled();
    expect(transactions.every(scroll => !scroll)).toBe(true);
    expect(Array.from(element.querySelectorAll('.editor-task-item > div')).map(node => node.textContent))
      .toEqual(['第二项', '第一项']);
    const completedCheckbox = element.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1];
    expect(completedCheckbox.checked).toBe(true);
    expect(completedCheckbox.ariaLabel).toBe('取消完成：第一项');
    expect(editor.state.selection.from).toBe(1);
    expect(editor.commands.undo()).toBe(true);
    expect(editor.state.doc.child(1).child(0).attrs.checked).toBe(false);
  });

  it('preserves the official read-only checkbox callback and rejection behavior', () => {
    const checked = vi.fn(() => false);
    const {element} = mountEditor(false, checked);
    const checkbox = element.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event('change', {bubbles: true}));
    expect(checked).toHaveBeenCalledOnce();
    expect(checkbox.checked).toBe(false);
  });

  it('keeps the caret inside the completed item when that item is moved', () => {
    const {editor} = mountEditor();
    let position = 0;
    editor.state.doc.descendants((node, pos) => {
      if (node.type.name === 'taskItem' && node.textContent === '第一项') position = pos;
    });
    editor.commands.setTextSelection(position + 3);
    editor.view.dispatch(editor.state.tr.setNodeMarkup(position, undefined, {checked: true}));
    expect(editor.state.selection.$from.parent.textContent).toBe('第一项');
    expect(editor.state.selection.$from.parentOffset).toBe(1);
  });
});
