import {keyboardEventMatchesShortcut} from './editorShortcuts';

export const AI_ASSISTANT_SHORTCUT_ACTIONS = ['focusInput'] as const;

export type AiAssistantShortcutAction = typeof AI_ASSISTANT_SHORTCUT_ACTIONS[number];
export type AiAssistantShortcutConfig = Record<AiAssistantShortcutAction, string>;

export const AI_ASSISTANT_SHORTCUT_LABELS: Record<AiAssistantShortcutAction, string> = {
  focusInput: '聚焦输入框',
};

export const DEFAULT_WINDOWS_AI_ASSISTANT_SHORTCUTS: AiAssistantShortcutConfig = {
  focusInput: 'Ctrl+Alt+KeyI',
};

export function normalizeAiAssistantShortcutConfig(value: unknown): AiAssistantShortcutConfig {
  const saved = value && typeof value === 'object'
    ? value as Partial<Record<AiAssistantShortcutAction, unknown>>
    : {};
  return Object.fromEntries(AI_ASSISTANT_SHORTCUT_ACTIONS.map(action => [
    action,
    typeof saved[action] === 'string'
      ? saved[action]
      : DEFAULT_WINDOWS_AI_ASSISTANT_SHORTCUTS[action],
  ])) as AiAssistantShortcutConfig;
}

export type AiAssistantShortcutKeyboardEvent = Pick<
  KeyboardEvent,
  'altKey' | 'code' | 'ctrlKey' | 'metaKey' | 'shiftKey'
>;

export function findAiAssistantShortcutAction(
  event: AiAssistantShortcutKeyboardEvent,
  config: AiAssistantShortcutConfig,
): AiAssistantShortcutAction | null {
  return AI_ASSISTANT_SHORTCUT_ACTIONS.find(action =>
    keyboardEventMatchesShortcut(event, config[action])
  ) ?? null;
}
