// @vitest-environment happy-dom

import {describe, expect, it} from 'vitest';
import {
  DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS,
  findDiaryListShortcutAction,
  findDiaryListShortcutConflict,
  isEditableShortcutTarget,
  normalizeDiaryListShortcutConfig,
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
  it('maps the default list shortcuts to their actions', () => {
    expect(findDiaryListShortcutAction(
      shortcutEvent('KeyN'),
      DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS,
    )).toBe('createDiary');
    expect(findDiaryListShortcutAction(
      shortcutEvent('KeyA', {altKey: true}),
      DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS,
    )).toBe('aiAssistant');
    expect(findDiaryListShortcutAction(
      shortcutEvent('KeyF'),
      DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS,
    )).toBe('search');
    expect(findDiaryListShortcutAction(
      shortcutEvent('Comma'),
      DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS,
    )).toBe('settings');
  });

  it('uses configured shortcuts and rejects unmatched shortcuts', () => {
    const shortcuts = {
      createDiary: 'Ctrl+KeyN',
      aiAssistant: 'Ctrl+Alt+KeyA',
      search: 'Ctrl+Alt+KeyS',
      settings: '',
    };

    expect(findDiaryListShortcutAction(
      shortcutEvent('KeyS', {altKey: true}),
      shortcuts,
    )).toBe('search');
    expect(findDiaryListShortcutAction(shortcutEvent('KeyF'), shortcuts)).toBeNull();
    expect(findDiaryListShortcutAction(shortcutEvent('Comma'), shortcuts)).toBeNull();
  });

  it('fills new actions when loading a saved legacy shortcut config', () => {
    expect(normalizeDiaryListShortcutConfig({
      search: 'Ctrl+Alt+KeyS',
      settings: '',
    })).toEqual({
      createDiary: 'Ctrl+KeyN',
      aiAssistant: 'Ctrl+Alt+KeyA',
      search: 'Ctrl+Alt+KeyS',
      settings: '',
    });
  });

  it('finds conflicts only between list actions', () => {
    expect(findDiaryListShortcutConflict(
      DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS,
      'settings',
      'Ctrl+KeyF',
    )).toBe('search');
    expect(findDiaryListShortcutConflict(
      DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS,
      'settings',
      'Ctrl+Alt+KeyS',
    )).toBeNull();
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
