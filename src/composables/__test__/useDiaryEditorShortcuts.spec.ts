// @vitest-environment happy-dom

import { describe, expect, it } from 'vitest'
import {
  isDiaryEditorTarget,
  isEditableFieldOutsideDiaryEditor,
} from '../useDiaryEditorShortcuts'

describe('diary editor shortcut target', () => {
  it('ignores editable controls outside the diary editor', () => {
    const input = document.createElement('input')
    document.body.append(input)
    expect(isEditableFieldOutsideDiaryEditor(input)).toBe(true)
  })

  it('allows contenteditable nodes inside ProseMirror', () => {
    const editor = document.createElement('div')
    editor.className = 'ProseMirror'
    const editable = document.createElement('div')
    editable.contentEditable = 'true'
    editor.append(editable)
    document.body.append(editor)
    expect(isEditableFieldOutsideDiaryEditor(editable)).toBe(false)
    expect(isDiaryEditorTarget(editable)).toBe(true)
  })

  it('allows non-editable targets', () => {
    expect(isEditableFieldOutsideDiaryEditor(document.createElement('button'))).toBe(false)
    expect(isEditableFieldOutsideDiaryEditor(null)).toBe(false)
    expect(isDiaryEditorTarget(document.createElement('button'))).toBe(false)
  })
})
