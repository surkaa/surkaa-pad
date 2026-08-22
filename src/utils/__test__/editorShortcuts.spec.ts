import { describe, expect, it } from 'vitest'
import {
  DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
  findEditorShortcutAction,
  findEditorShortcutConflict,
  findReassignedNativeEditorShortcut,
  formatEditorShortcut,
  keyboardEventMatchesShortcut,
  normalizeEditorShortcutConfig,
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
    expect(formatEditorShortcut('Ctrl+Comma')).toBe('Ctrl+,')
    expect(findEditorShortcutAction(event, DEFAULT_WINDOWS_EDITOR_SHORTCUTS))
      .toBe('insertPhoto')
  })

  it('provides the requested default toolbar shortcuts', () => {
    expect(DEFAULT_WINDOWS_EDITOR_SHORTCUTS).toMatchObject({
      bold: 'Ctrl+KeyB',
      underline: 'Ctrl+KeyU',
      strike: 'Ctrl+Shift+KeyS',
      heading1: 'Ctrl+Digit1',
      heading2: 'Ctrl+Digit2',
      heading3: 'Ctrl+Digit3',
      summary: 'Ctrl+Alt+KeyS',
      taskList: 'Ctrl+KeyT',
    })
    expect(findEditorShortcutAction(
      keyEvent({ctrlKey: true, code: 'Digit2'}),
      DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
    )).toBe('heading2')
  })

  it('fills new toolbar shortcuts into an existing attachment-only config', () => {
    const normalized = normalizeEditorShortcutConfig({
      insertPhoto: 'Ctrl+Alt+KeyI',
      insertAudio: '',
    })

    expect(normalized.bold).toBe('Ctrl+KeyB')
    expect(normalized.insertPhoto).toBe('Ctrl+Alt+KeyI')
    expect(normalized.insertAudio).toBe('')
    expect(normalized.insertFile).toBe('Ctrl+Alt+KeyF')
  })

  it('detects a native Tiptap shortcut that must be suppressed after reassignment', () => {
    const reassigned = {...DEFAULT_WINDOWS_EDITOR_SHORTCUTS, bold: 'Ctrl+Alt+KeyB'}
    expect(findReassignedNativeEditorShortcut(
      keyEvent({ctrlKey: true, code: 'KeyB'}),
      reassigned,
    )).toBe('bold')
    expect(findReassignedNativeEditorShortcut(
      keyEvent({ctrlKey: true, code: 'KeyB'}),
      DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
    )).toBeNull()
    expect(findReassignedNativeEditorShortcut(
      keyEvent({ctrlKey: true, altKey: true, code: 'Digit1'}),
      DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
    )).toBe('heading1')
  })

  it('does not resolve cleared or unmatched shortcuts', () => {
    const shortcuts = {...DEFAULT_WINDOWS_EDITOR_SHORTCUTS, insertPhoto: ''}
    expect(findEditorShortcutAction(
      keyEvent({ctrlKey: true, altKey: true}),
      shortcuts,
    )).toBeNull()
    expect(findEditorShortcutAction(
      keyEvent({ctrlKey: true, altKey: true, code: 'KeyZ'}),
      DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
    )).toBeNull()
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
