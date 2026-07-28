export const EDITOR_SHORTCUT_ACTIONS = [
  'insertPhoto',
  'insertAudio',
  'audioRecording',
  'insertVideo',
  'insertFile',
] as const

export type EditorShortcutAction = typeof EDITOR_SHORTCUT_ACTIONS[number]
export type EditorShortcutConfig = Record<EditorShortcutAction, string>

export const EDITOR_SHORTCUT_LABELS: Record<EditorShortcutAction, string> = {
  insertPhoto: '照片',
  insertAudio: '音频',
  audioRecording: '录音',
  insertVideo: '视频',
  insertFile: '文件',
}

export const DEFAULT_WINDOWS_EDITOR_SHORTCUTS: EditorShortcutConfig = {
  insertPhoto: 'Ctrl+Alt+KeyP',
  insertAudio: 'Ctrl+Alt+KeyA',
  audioRecording: 'Ctrl+Alt+KeyR',
  insertVideo: 'Ctrl+Alt+KeyV',
  insertFile: 'Ctrl+Alt+KeyF',
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
