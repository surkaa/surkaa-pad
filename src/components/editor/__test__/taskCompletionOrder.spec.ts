// @vitest-environment happy-dom
import {describe, expect, it} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import TaskItem from '@tiptap/extension-task-item';
import TaskList from '@tiptap/extension-task-list';
import {TaskCompletionOrder} from '../taskCompletionOrder';

interface TaskInput {
  text: string;
  checked: boolean;
}

describe('task completion order', () => {
  it('moves a completed item immediately after the final incomplete item', () => {
    const editor = createEditor([
      {text: '第一项', checked: false},
      {text: '第二项', checked: false},
      {text: '已完成', checked: true},
    ]);

    completeTask(editor, '第一项');

    expect(readTasks(editor)).toEqual([
      {text: '第二项', checked: false},
      {text: '第一项', checked: true},
      {text: '已完成', checked: true},
    ]);
    editor.destroy();
  });

  it('normalizes a newly completed item that was below older completed items', () => {
    const editor = createEditor([
      {text: '未完成', checked: false},
      {text: '旧完成项', checked: true},
      {text: '刚完成', checked: false},
      {text: '末尾完成项', checked: true},
    ]);

    completeTask(editor, '刚完成');

    expect(readTasks(editor).map(task => task.text)).toEqual([
      '未完成',
      '刚完成',
      '旧完成项',
      '末尾完成项',
    ]);
    editor.destroy();
  });

  it('keeps the order when the list becomes fully completed', () => {
    const editor = createEditor([
      {text: '原完成项', checked: true},
      {text: '最后未完成项', checked: false},
    ]);

    completeTask(editor, '最后未完成项');

    expect(readTasks(editor)).toEqual([
      {text: '原完成项', checked: true},
      {text: '最后未完成项', checked: true},
    ]);
    editor.destroy();
  });

  it('does not move an item when completion is cancelled', () => {
    const editor = createEditor([
      {text: '未完成', checked: false},
      {text: '取消完成', checked: true},
    ]);

    setTaskChecked(editor, '取消完成', false);

    expect(readTasks(editor)).toEqual([
      {text: '未完成', checked: false},
      {text: '取消完成', checked: false},
    ]);
    editor.destroy();
  });

  it('only reorders items inside the list that was changed', () => {
    const editor = new Editor({
      extensions: [StarterKit, TaskList, TaskItem, TaskCompletionOrder],
      content: {
        type: 'doc',
        content: [
          taskListNode([
            {text: '列表一第一项', checked: false},
            {text: '列表一第二项', checked: false},
          ]),
          {type: 'paragraph', content: [{type: 'text', text: '列表分隔'}]},
          taskListNode([
            {text: '列表二未完成', checked: false},
            {text: '列表二已完成', checked: true},
          ]),
          {type: 'paragraph'},
        ],
      },
    });

    completeTask(editor, '列表一第一项');

    expect(readTaskLists(editor)).toEqual([
      [
        {text: '列表一第二项', checked: false},
        {text: '列表一第一项', checked: true},
      ],
      [
        {text: '列表二未完成', checked: false},
        {text: '列表二已完成', checked: true},
      ],
    ]);
    editor.destroy();
  });

  it('restores both completion and order with one undo', () => {
    const editor = createEditor([
      {text: '第一项', checked: false},
      {text: '第二项', checked: false},
      {text: '完成项', checked: true},
    ]);
    const original = editor.getJSON();

    completeTask(editor, '第一项');
    expect(editor.commands.undo()).toBe(true);

    expect(editor.getJSON()).toEqual(original);
    editor.destroy();
  });
});

function createEditor(tasks: TaskInput[]): Editor {
  return new Editor({
    extensions: [StarterKit, TaskList, TaskItem, TaskCompletionOrder],
    content: {
      type: 'doc',
      content: [
        taskListNode(tasks),
        {type: 'paragraph'},
      ],
    },
  });
}

function taskListNode(tasks: TaskInput[]) {
  return {
    type: 'taskList',
    content: tasks.map(task => ({
      type: 'taskItem',
      attrs: {checked: task.checked},
      content: [{
        type: 'paragraph',
        content: [{type: 'text', text: task.text}],
      }],
    })),
  };
}

function completeTask(editor: Editor, text: string) {
  setTaskChecked(editor, text, true);
}

function setTaskChecked(editor: Editor, text: string, checked: boolean) {
  let taskPosition: number | null = null;
  editor.state.doc.descendants((node, position) => {
    if (taskPosition === null && node.type.name === 'taskItem' && node.textContent === text) {
      taskPosition = position;
      return false;
    }
    return taskPosition === null;
  });
  if (taskPosition === null) throw new Error(`未找到待办：${text}`);
  editor.view.dispatch(editor.state.tr.setNodeMarkup(taskPosition, undefined, {checked}));
}

function readTasks(editor: Editor): TaskInput[] {
  const result: TaskInput[] = [];
  editor.state.doc.descendants(node => {
    if (node.type.name === 'taskItem') {
      result.push({text: node.textContent, checked: node.attrs.checked === true});
      return false;
    }
    return true;
  });
  return result;
}

function readTaskLists(editor: Editor): TaskInput[][] {
  const lists: TaskInput[][] = [];
  editor.state.doc.forEach(node => {
    if (node.type.name !== 'taskList') return;
    const tasks: TaskInput[] = [];
    node.forEach(task => tasks.push({
      text: task.textContent,
      checked: task.attrs.checked === true,
    }));
    lists.push(tasks);
  });
  return lists;
}
