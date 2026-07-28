import type { Ref } from 'vue'
import { useEventListener } from '@vueuse/core'
import { platform } from '@tauri-apps/plugin-os'
import { useRoute } from 'vue-router'
import {
  findEditorShortcutAction,
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
    if (!action) return

    event.preventDefault()
    event.stopPropagation()
    options.showToolbarPanel.value = false
    options.handlers[action]()
  }, { capture: true })
}
