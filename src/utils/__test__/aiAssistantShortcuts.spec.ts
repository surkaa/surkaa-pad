import {describe, expect, it} from 'vitest';
import {
  DEFAULT_WINDOWS_AI_ASSISTANT_SHORTCUTS,
  findAiAssistantShortcutAction,
  normalizeAiAssistantShortcutConfig,
} from '../aiAssistantShortcuts';

function shortcutEvent(code: string, overrides: Partial<KeyboardEvent> = {}) {
  return {
    code,
    ctrlKey: true,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...overrides,
  } as KeyboardEvent;
}

describe('AI assistant shortcuts', () => {
  it('maps Ctrl+Alt+I to focusing the question input', () => {
    expect(findAiAssistantShortcutAction(
      shortcutEvent('KeyI', {altKey: true}),
      DEFAULT_WINDOWS_AI_ASSISTANT_SHORTCUTS,
    )).toBe('focusInput');
    expect(findAiAssistantShortcutAction(
      shortcutEvent('KeyI'),
      DEFAULT_WINDOWS_AI_ASSISTANT_SHORTCUTS,
    )).toBeNull();
  });

  it('preserves a saved shortcut and fills an invalid config', () => {
    expect(normalizeAiAssistantShortcutConfig({focusInput: 'Ctrl+KeyL'}))
      .toEqual({focusInput: 'Ctrl+KeyL'});
    expect(normalizeAiAssistantShortcutConfig(null))
      .toEqual(DEFAULT_WINDOWS_AI_ASSISTANT_SHORTCUTS);
  });
});
