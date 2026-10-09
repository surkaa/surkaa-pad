import type {Editor} from '@tiptap/vue-3';

export const EDITOR_FORMATTING_ACTIONS = [
  'bold',
  'underline',
  'strike',
  'heading1',
  'heading2',
  'heading3',
  'heading4',
  'heading5',
  'taskList',
  'summary',
] as const;

export type EditorFormattingAction = typeof EDITOR_FORMATTING_ACTIONS[number];

export const EDITOR_TOOLBAR_ACTIONS = [
  ...EDITOR_FORMATTING_ACTIONS,
  'undo',
  'redo',
] as const;

export type EditorToolbarAction = typeof EDITOR_TOOLBAR_ACTIONS[number];

export const EDITOR_TOOLBAR_LABELS: Record<EditorToolbarAction, string> = {
  bold: '加粗',
  underline: '下划线',
  strike: '删除线',
  heading1: '一级标题',
  heading2: '二级标题',
  heading3: '三级标题',
  heading4: '四级标题',
  heading5: '五级标题',
  taskList: '待办列表',
  summary: '折叠内容',
  undo: '撤销',
  redo: '重做',
};

export const EDITOR_TOOLBAR_ICONS: Record<EditorToolbarAction, string> = {
  bold: 'format_bold',
  underline: 'format_underlined',
  strike: 'strikethrough_s',
  heading1: 'looks_one',
  heading2: 'looks_two',
  heading3: 'looks_3',
  heading4: 'looks_4',
  heading5: 'looks_5',
  taskList: 'checklist',
  summary: 'unfold_more',
  undo: 'undo',
  redo: 'redo',
};

export const DEFAULT_EDITOR_TOOLBAR_ORDER: EditorToolbarAction[] = [...EDITOR_TOOLBAR_ACTIONS];

export function normalizeEditorToolbarOrder(value: unknown): EditorToolbarAction[] {
  const result: EditorToolbarAction[] = [];
  if (Array.isArray(value)) {
    for (const candidate of value) {
      if (
        typeof candidate === 'string'
        && EDITOR_TOOLBAR_ACTIONS.includes(candidate as EditorToolbarAction)
        && !result.includes(candidate as EditorToolbarAction)
      ) {
        result.push(candidate as EditorToolbarAction);
      }
    }
  }

  // 旧版本没有 H4/H5；升级时将它们放在 H3 后，而不是追加到撤销/重做之后。
  for (const [action, previous] of [
    ['heading4', 'heading3'],
    ['heading5', 'heading4'],
  ] as const) {
    if (result.includes(action)) continue;
    const previousIndex = result.indexOf(previous);
    if (previousIndex >= 0) result.splice(previousIndex + 1, 0, action);
  }

  for (const action of EDITOR_TOOLBAR_ACTIONS) {
    if (!result.includes(action)) result.push(action);
  }
  return result;
}

export function moveEditorToolbarAction(
  order: readonly EditorToolbarAction[],
  action: EditorToolbarAction,
  direction: -1 | 1,
): EditorToolbarAction[] {
  const next = normalizeEditorToolbarOrder(order);
  const currentIndex = next.indexOf(action);
  const targetIndex = currentIndex + direction;
  if (currentIndex < 0 || targetIndex < 0 || targetIndex >= next.length) return next;
  [next[currentIndex], next[targetIndex]] = [next[targetIndex], next[currentIndex]];
  return next;
}

export function reorderEditorToolbarAction(
  order: readonly EditorToolbarAction[],
  action: EditorToolbarAction,
  insertionIndex: number,
): EditorToolbarAction[] {
  const next = normalizeEditorToolbarOrder(order);
  const currentIndex = next.indexOf(action);
  if (currentIndex < 0) return next;
  next.splice(currentIndex, 1);
  const targetIndex = Math.min(
    next.length,
    Math.max(0, Math.trunc(insertionIndex)),
  );
  next.splice(targetIndex, 0, action);
  return next;
}

export function runEditorToolbarAction(
  editor: Editor | null | undefined,
  action: EditorToolbarAction,
  openSummary: () => void,
): boolean {
  if (!editor) return false;
  if (action === 'summary') {
    openSummary();
    return true;
  }
  if (action === 'undo') return editor.chain().undo().run();
  if (action === 'redo') return editor.chain().redo().run();

  const chain = editor.chain().focus();
  switch (action) {
    case 'bold': return chain.toggleBold().run();
    case 'underline': return chain.toggleUnderline().run();
    case 'strike': return chain.toggleStrike().run();
    case 'heading1': return chain.toggleHeading({level: 1}).run();
    case 'heading2': return chain.toggleHeading({level: 2}).run();
    case 'heading3': return chain.toggleHeading({level: 3}).run();
    case 'heading4': return chain.toggleHeading({level: 4}).run();
    case 'heading5': return chain.toggleHeading({level: 5}).run();
    case 'taskList': return chain.toggleTaskList().run();
  }
}
