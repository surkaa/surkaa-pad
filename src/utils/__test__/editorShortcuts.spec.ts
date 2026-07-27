import { describe, expect, it } from 'vitest'
import {
  DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
  findEditorShortcutConflict,
  formatEditorShortcut,
  keyboardEventMatchesShortcut,
  shortcutFromKeyboardEvent,
} from '../editorShortcuts'

function keyEvent(overrides: Partial<KeyboardEvent> = {}) {
  return {
    altKey: false,
    code: 'KeyP',
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    ...overrides,
  } as KeyboardEvent
}

describe('editor shortcuts', () => {
  it('normalizes Windows key combinations in a stable order', () => {
    expect(shortcutFromKeyboardEvent(keyEvent({ctrlKey: true, altKey: true})))
      .toBe('Ctrl+Alt+KeyP')
    expect(shortcutFromKeyboardEvent(keyEvent({ctrlKey: true, shiftKey: true})))
      .toBe('Ctrl+Shift+KeyP')
  })

  it('rejects bare keys, modifier-only input and the Windows key', () => {
    expect(shortcutFromKeyboardEvent(keyEvent())).toBeNull()
    expect(shortcutFromKeyboardEvent(keyEvent({ctrlKey: true, code: 'ControlLeft'}))).toBeNull()
    expect(shortcutFromKeyboardEvent(keyEvent({ctrlKey: true, metaKey: true}))).toBeNull()
  })

  it('matches and formats configured shortcuts', () => {
    const event = keyEvent({ctrlKey: true, altKey: true})
    expect(keyboardEventMatchesShortcut(event, 'Ctrl+Alt+KeyP')).toBe(true)
    expect(formatEditorShortcut('Ctrl+Alt+KeyP')).toBe('Ctrl+Alt+P')
  })

  it('detects duplicate assignments but permits clearing', () => {
    expect(findEditorShortcutConflict(
      DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
      'insertAudio',
      'Ctrl+Alt+KeyP',
    )).toBe('insertPhoto')
    expect(findEditorShortcutConflict(
      DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
      'insertAudio',
      '',
    )).toBeNull()
  })
})
