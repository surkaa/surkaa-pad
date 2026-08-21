export const EDITOR_TOOLBAR_ACTIONS = [
  'bold',
  'underline',
  'strike',
  'heading1',
  'heading2',
  'heading3',
  'taskList',
] as const;

export type EditorToolbarAction = typeof EDITOR_TOOLBAR_ACTIONS[number];

export const EDITOR_TOOLBAR_LABELS: Record<EditorToolbarAction, string> = {
  bold: '加粗',
  underline: '下划线',
  strike: '删除线',
  heading1: '一级标题',
  heading2: '二级标题',
  heading3: '三级标题',
  taskList: '待办列表',
};

export const EDITOR_TOOLBAR_ICONS: Record<EditorToolbarAction, string> = {
  bold: 'format_bold',
  underline: 'format_underlined',
  strike: 'strikethrough_s',
  heading1: 'looks_one',
  heading2: 'looks_two',
  heading3: 'looks_3',
  taskList: 'checklist',
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
