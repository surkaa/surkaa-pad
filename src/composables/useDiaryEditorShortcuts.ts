import type { Ref } from 'vue'
import { useEventListener } from '@vueuse/core'
import { platform } from '@tauri-apps/plugin-os'
import { useRoute } from 'vue-router'
import {
  findEditorShortcutAction,
  findReassignedNativeEditorShortcut,
  isEditorToolbarShortcutAction,
  type EditorShortcutAction,
  type EditorShortcutConfig,
} from '../utils/editorShortcuts'

type EditorShortcutHandlers = Record<EditorShortcutAction, () => void>

interface DiaryEditorShortcutOptions {
  shortcuts: Ref<EditorShortcutConfig>
  showToolbarPanel: Ref<boolean>
  isInteractionBlocked: () => boolean
  handlers: EditorShortcutHandlers
}

export function isEditableFieldOutsideDiaryEditor(target: EventTarget | null) {
  const element = target instanceof Element ? target : null
  if (!element || element.closest('.ProseMirror')) return false
  return Boolean(element.closest('input, textarea, select, [contenteditable="true"]'))
}

export function isDiaryEditorTarget(target: EventTarget | null) {
  const element = target instanceof Element ? target : null
  return Boolean(element?.closest('.ProseMirror'))
}

export function useDiaryEditorShortcuts(options: DiaryEditorShortcutOptions) {
  if (platform() !== 'windows') return

  const route = useRoute()
  useEventListener(window, 'keydown', (event: KeyboardEvent) => {
    if (
      event.repeat
      || event.isComposing
      || route.name !== 'DiaryDetail'
      || isEditableFieldOutsideDiaryEditor(event.target)
      || options.isInteractionBlocked()
    ) return

    const action = findEditorShortcutAction(event, options.shortcuts.value)
    const targetsEditor = isDiaryEditorTarget(event.target)
    if (!action) {
      if (
        targetsEditor
        && findReassignedNativeEditorShortcut(event, options.shortcuts.value)
      ) {
        event.preventDefault()
        event.stopPropagation()
      }
      return
    }
    if (isEditorToolbarShortcutAction(action) && !targetsEditor) return

    event.preventDefault()
    event.stopPropagation()
    options.showToolbarPanel.value = false
    options.handlers[action]()
  }, { capture: true })
}
