import {
  EDITOR_FORMATTING_ACTIONS,
  EDITOR_TOOLBAR_LABELS,
  type EditorFormattingAction,
} from './editorToolbar'

export const EDITOR_ATTACHMENT_SHORTCUT_ACTIONS = [
  'insertPhoto',
  'insertAudio',
  'audioRecording',
  'insertVideo',
  'insertFile',
] as const

export const EDITOR_SHORTCUT_ACTIONS = [
  ...EDITOR_FORMATTING_ACTIONS,
  ...EDITOR_ATTACHMENT_SHORTCUT_ACTIONS,
] as const

export type EditorShortcutAction = typeof EDITOR_SHORTCUT_ACTIONS[number]
export type EditorShortcutConfig = Record<EditorShortcutAction, string>

export const EDITOR_SHORTCUT_LABELS: Record<EditorShortcutAction, string> = {
  ...EDITOR_TOOLBAR_LABELS,
  insertPhoto: '照片',
  insertAudio: '音频',
  audioRecording: '录音',
  insertVideo: '视频',
  insertFile: '文件',
}

export const DEFAULT_WINDOWS_EDITOR_SHORTCUTS: EditorShortcutConfig = {
  bold: 'Ctrl+KeyB',
  underline: 'Ctrl+KeyU',
  strike: 'Ctrl+Shift+KeyS',
  heading1: 'Ctrl+Digit1',
  heading2: 'Ctrl+Digit2',
  heading3: 'Ctrl+Digit3',
  heading4: 'Ctrl+Digit4',
  heading5: 'Ctrl+Digit5',
  bulletList: 'Ctrl+Shift+Digit8',
  taskList: 'Ctrl+KeyT',
  summary: 'Ctrl+Alt+KeyS',
  table: 'Ctrl+Shift+KeyT',
  insertPhoto: 'Ctrl+Alt+KeyP',
  insertAudio: 'Ctrl+Alt+KeyA',
  audioRecording: 'Ctrl+Alt+KeyR',
  insertVideo: 'Ctrl+Alt+KeyV',
  insertFile: 'Ctrl+Alt+KeyF',
}

/** Tiptap 内置的工具栏快捷键；用户改键后需要拦截这些旧组合。 */
export const NATIVE_EDITOR_SHORTCUTS: Partial<Record<EditorFormattingAction, readonly string[]>> = {
  bold: ['Ctrl+KeyB', 'Ctrl+Shift+KeyB'],
  underline: ['Ctrl+KeyU', 'Ctrl+Shift+KeyU'],
  strike: ['Ctrl+Shift+KeyS'],
  heading1: ['Ctrl+Alt+Digit1'],
  heading2: ['Ctrl+Alt+Digit2'],
  heading3: ['Ctrl+Alt+Digit3'],
  heading4: ['Ctrl+Alt+Digit4'],
  heading5: ['Ctrl+Alt+Digit5'],
  bulletList: ['Ctrl+Shift+Digit8'],
}

const MODIFIER_CODES = new Set([
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'ShiftLeft',
  'ShiftRight',
  'MetaLeft',
  'MetaRight',
])

export type ShortcutKeyboardEvent = Pick<
  KeyboardEvent,
  'altKey' | 'code' | 'ctrlKey' | 'metaKey' | 'shiftKey'
>

export function shortcutFromKeyboardEvent(event: ShortcutKeyboardEvent): string | null {
  if (event.metaKey || MODIFIER_CODES.has(event.code)) return null
  if (!event.ctrlKey && !event.altKey) return null

  return [
    event.ctrlKey ? 'Ctrl' : '',
    event.altKey ? 'Alt' : '',
    event.shiftKey ? 'Shift' : '',
    event.code,
  ].filter(Boolean).join('+')
}

export function keyboardEventMatchesShortcut(
  event: ShortcutKeyboardEvent,
  shortcut: string,
): boolean {
  return Boolean(shortcut) && shortcutFromKeyboardEvent(event) === shortcut
}

export function findEditorShortcutAction(
  event: ShortcutKeyboardEvent,
  config: EditorShortcutConfig,
): EditorShortcutAction | null {
  return EDITOR_SHORTCUT_ACTIONS.find(action =>
    keyboardEventMatchesShortcut(event, config[action])
  ) ?? null
}

export function isEditorToolbarShortcutAction(
  action: EditorShortcutAction,
): action is EditorFormattingAction {
  return EDITOR_FORMATTING_ACTIONS.includes(action as EditorFormattingAction)
}

export function findReassignedNativeEditorShortcut(
  event: ShortcutKeyboardEvent,
  config: EditorShortcutConfig,
): EditorFormattingAction | null {
  for (const action of EDITOR_FORMATTING_ACTIONS) {
    const nativeShortcuts = NATIVE_EDITOR_SHORTCUTS[action]
    const matchedShortcut = nativeShortcuts?.find(shortcut =>
      keyboardEventMatchesShortcut(event, shortcut)
    )
    if (matchedShortcut && config[action] !== matchedShortcut) return action
  }
  return null
}

export function normalizeEditorShortcutConfig(value: unknown): EditorShortcutConfig {
  const source = value && typeof value === 'object'
    ? value as Partial<Record<EditorShortcutAction, unknown>>
    : {}
  const normalized = Object.fromEntries(EDITOR_SHORTCUT_ACTIONS.map(action => [
    action,
    typeof source[action] === 'string'
      ? source[action]
      : DEFAULT_WINDOWS_EDITOR_SHORTCUTS[action],
  ])) as EditorShortcutConfig
  // 新增默认组合不能抢占旧配置中其他操作的自定义快捷键；显式清空也仍然保留。
  if (typeof source.table !== 'string' && EDITOR_SHORTCUT_ACTIONS.some(action =>
    action !== 'table' && normalized[action] === normalized.table,
  )) normalized.table = ''
  return normalized
}

export function formatEditorShortcut(shortcut: string): string {
  if (!shortcut) return '未设置'
  return shortcut
    .replace(/Key([A-Z])/g, '$1')
    .replace(/Digit([0-9])/g, '$1')
    .replace(/Arrow/g, '')
    .replace(/Comma/g, ',')
}

export function findEditorShortcutConflict(
  config: EditorShortcutConfig,
  action: EditorShortcutAction,
  shortcut: string,
): EditorShortcutAction | null {
  if (!shortcut) return null
  return EDITOR_SHORTCUT_ACTIONS.find(candidate =>
    candidate !== action && config[candidate] === shortcut
  ) ?? null
}
