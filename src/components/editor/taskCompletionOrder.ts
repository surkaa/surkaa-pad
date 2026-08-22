import {Extension} from '@tiptap/vue-3';
import type {Node as ProseMirrorNode} from '@tiptap/pm/model';
import {Plugin, PluginKey, type EditorState, type Transaction} from '@tiptap/pm/state';

const taskCompletionOrderKey = new PluginKey('taskCompletionOrder');

export function createTaskCompletionOrderTransaction(
  oldState: EditorState,
  newState: EditorState,
): Transaction | null {
  const completedPosition = findNewlyCompletedTask(oldState.doc, newState.doc);
  if (completedPosition === null) return null;

  const completedTask = newState.doc.nodeAt(completedPosition);
  if (!completedTask) return null;
  const resolved = newState.doc.resolve(completedPosition);
  const taskList = resolved.parent;
  if (taskList.type.name !== 'taskList') return null;

  let lastIncompleteIndex = -1;
  let positionAfterLastIncomplete = resolved.start();
  let childPosition = resolved.start();
  taskList.forEach((task, _offset, index) => {
    childPosition += task.nodeSize;
    if (task.attrs.checked !== true) {
      lastIncompleteIndex = index;
      positionAfterLastIncomplete = childPosition;
    }
  });

  // 全部完成时没有排序边界，保留用户现有顺序。
  if (lastIncompleteIndex < 0) return null;

  let insertPosition = positionAfterLastIncomplete;
  if (completedPosition < insertPosition) insertPosition -= completedTask.nodeSize;
  if (insertPosition === completedPosition) return null;

  return newState.tr
    .delete(completedPosition, completedPosition + completedTask.nodeSize)
    .insert(insertPosition, completedTask);
}

function findNewlyCompletedTask(
  oldDocument: ProseMirrorNode,
  newDocument: ProseMirrorNode,
): number | null {
  let result: number | null = null;
  newDocument.descendants((node, position) => {
    if (result !== null || node.type.name !== 'taskItem' || node.attrs.checked !== true) {
      return result === null;
    }
    const oldNode = oldDocument.nodeAt(position);
    if (oldNode?.type.name === 'taskItem' && oldNode.attrs.checked !== true) {
      result = position;
    }
    return false;
  });
  return result;
}

export const TaskCompletionOrder = Extension.create({
  name: 'taskCompletionOrder',

  addProseMirrorPlugins() {
    return [new Plugin({
      key: taskCompletionOrderKey,
      appendTransaction(transactions, oldState, newState) {
        if (!transactions.some(transaction => transaction.docChanged)) return null;
        if (transactions.some(transaction => transaction.getMeta(taskCompletionOrderKey))) {
          return null;
        }
        return createTaskCompletionOrderTransaction(oldState, newState)
          ?.setMeta(taskCompletionOrderKey, true) ?? null;
      },
    })];
  },
});
