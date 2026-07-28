import {platform} from '@tauri-apps/plugin-os';
import {useEventListener} from '@vueuse/core';
import {useRoute} from 'vue-router';
import type {Ref} from 'vue';
import {
  findDiaryListShortcutAction,
  isEditableShortcutTarget,
  type DiaryListShortcutAction,
  type DiaryListShortcutConfig,
} from '../utils/diaryListShortcuts';

type DiaryListShortcutHandlers = Record<DiaryListShortcutAction, () => void>;

export function useDiaryListShortcuts(
  shortcuts: Ref<DiaryListShortcutConfig>,
  handlers: DiaryListShortcutHandlers,
) {
  if (platform() !== 'windows') return;

  const route = useRoute();
  useEventListener(window, 'keydown', (event: KeyboardEvent) => {
    if (
      event.repeat
      || event.isComposing
      || route.name !== 'DiaryList'
      || isEditableShortcutTarget(event.target)
    ) return;

    const action = findDiaryListShortcutAction(event, shortcuts.value);
    if (!action) return;

    event.preventDefault();
    event.stopPropagation();
    handlers[action]();
  }, {capture: true});
}
