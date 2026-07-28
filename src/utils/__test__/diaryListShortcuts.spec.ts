// @vitest-environment happy-dom

import {describe, expect, it} from 'vitest';
import {
  findDiaryListShortcutAction,
  isEditableShortcutTarget,
} from '../diaryListShortcuts';

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

describe('diary list shortcuts', () => {
  it('maps Ctrl+F and Ctrl+Comma to list navigation', () => {
    expect(findDiaryListShortcutAction(shortcutEvent('KeyF'))).toBe('search');
    expect(findDiaryListShortcutAction(shortcutEvent('Comma'))).toBe('settings');
  });

  it('rejects modified and unrelated shortcuts', () => {
    expect(findDiaryListShortcutAction(shortcutEvent('KeyF', {shiftKey: true}))).toBeNull();
    expect(findDiaryListShortcutAction(shortcutEvent('KeyS'))).toBeNull();
  });

  it('recognizes editable shortcut targets', () => {
    const input = document.createElement('input');
    const button = document.createElement('button');
    document.body.append(input, button);

    expect(isEditableShortcutTarget(input)).toBe(true);
    expect(isEditableShortcutTarget(button)).toBe(false);
    expect(isEditableShortcutTarget(null)).toBe(false);
  });
});
