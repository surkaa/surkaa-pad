import {platform} from '@tauri-apps/plugin-os';
import {useEventListener} from '@vueuse/core';
import type {Ref} from 'vue';
import {useRoute} from 'vue-router';
import {
  findAiAssistantShortcutAction,
  type AiAssistantShortcutAction,
  type AiAssistantShortcutConfig,
} from '../utils/aiAssistantShortcuts';

type AiAssistantShortcutHandlers = Record<AiAssistantShortcutAction, () => void>;

export function useAiAssistantShortcuts(
  shortcuts: Ref<AiAssistantShortcutConfig>,
  handlers: AiAssistantShortcutHandlers,
) {
  if (platform() !== 'windows') return;

  const route = useRoute();
  useEventListener(window, 'keydown', (event: KeyboardEvent) => {
    if (event.repeat || event.isComposing || route.name !== 'AiAssistant') return;

    const action = findAiAssistantShortcutAction(event, shortcuts.value);
    if (!action) return;

    event.preventDefault();
    event.stopPropagation();
    handlers[action]();
  }, {capture: true});
}
