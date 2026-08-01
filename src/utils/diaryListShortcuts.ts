import {keyboardEventMatchesShortcut} from './editorShortcuts';

export const DIARY_LIST_SHORTCUT_ACTIONS = [
  'createDiary',
  'aiAssistant',
  'search',
  'settings',
] as const;

export type DiaryListShortcutAction = typeof DIARY_LIST_SHORTCUT_ACTIONS[number];
export type DiaryListShortcutConfig = Record<DiaryListShortcutAction, string>;

export const DIARY_LIST_SHORTCUT_LABELS: Record<DiaryListShortcutAction, string> = {
  createDiary: '新建日记',
  aiAssistant: '打开 AI 助手',
  search: '搜索日记',
  settings: '打开设置',
};

export const DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS: DiaryListShortcutConfig = {
  createDiary: 'Ctrl+KeyN',
  aiAssistant: 'Ctrl+Alt+KeyA',
  search: 'Ctrl+KeyF',
  settings: 'Ctrl+Comma',
};

export function normalizeDiaryListShortcutConfig(value: unknown): DiaryListShortcutConfig {
  const saved = value && typeof value === 'object'
    ? value as Partial<Record<DiaryListShortcutAction, unknown>>
    : {};
  return Object.fromEntries(DIARY_LIST_SHORTCUT_ACTIONS.map(action => [
    action,
    typeof saved[action] === 'string'
      ? saved[action]
      : DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS[action],
  ])) as DiaryListShortcutConfig;
}

export type DiaryListShortcutKeyboardEvent = Pick<
  KeyboardEvent,
  'altKey' | 'code' | 'ctrlKey' | 'metaKey' | 'shiftKey'
>;

export function findDiaryListShortcutAction(
  event: DiaryListShortcutKeyboardEvent,
  config: DiaryListShortcutConfig,
): DiaryListShortcutAction | null {
  return DIARY_LIST_SHORTCUT_ACTIONS.find(action =>
    keyboardEventMatchesShortcut(event, config[action])
  ) ?? null;
}

export function findDiaryListShortcutConflict(
  config: DiaryListShortcutConfig,
  action: DiaryListShortcutAction,
  shortcut: string,
): DiaryListShortcutAction | null {
  if (!shortcut) return null;
  return DIARY_LIST_SHORTCUT_ACTIONS.find(candidate =>
    candidate !== action && config[candidate] === shortcut
  ) ?? null;
}

export function isEditableShortcutTarget(target: EventTarget | null): boolean {
  const element = target instanceof Element ? target : null;
  return Boolean(element?.closest('input, textarea, select, [contenteditable="true"]'));
}
