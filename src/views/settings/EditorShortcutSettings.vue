<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">编辑器快捷键</div>
    <q-list bordered separator class="pad-card">
      <q-item
        v-for="action in EDITOR_SHORTCUT_ACTIONS"
        :key="action"
        class="settings-item shortcut-settings-item"
      >
        <q-item-section avatar class="settings-icon-section">
          <q-icon :name="editorShortcutIcons[action]"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium">
            {{ EDITOR_SHORTCUT_LABELS[action] }}
          </q-item-label>
          <q-item-label caption class="desc-text">仅在日记编辑页面生效</q-item-label>
        </q-item-section>
        <q-item-section side>
          <ShortcutRecorder
            :model-value="editorShortcuts[action]"
            :label="EDITOR_SHORTCUT_LABELS[action]"
            @update:model-value="shortcut => updateEditorShortcut(action, shortcut)"
            @invalid="notifyInvalidShortcut"
          />
        </q-item-section>
      </q-item>
    </q-list>
  </section>
</template>

<script setup lang="ts">
import {useQuasar} from 'quasar';
import ShortcutRecorder from '../../components/ShortcutRecorder.vue';
import {useConfigStore} from '../../stores/config';
import {
  EDITOR_SHORTCUT_ACTIONS,
  EDITOR_SHORTCUT_LABELS,
  findEditorShortcutConflict,
  type EditorShortcutAction,
} from '../../utils/editorShortcuts';

const $q = useQuasar();
const configStore = useConfigStore();
const editorShortcuts = configStore.useTauriConfig('windows_editor_shortcuts');
const editorShortcutIcons: Record<EditorShortcutAction, string> = {
  insertPhoto: 'image',
  insertAudio: 'audiotrack',
  audioRecording: 'mic',
  insertVideo: 'video_library',
  insertFile: 'attach_file',
};

function updateEditorShortcut(action: EditorShortcutAction, shortcut: string) {
  const conflict = findEditorShortcutConflict(editorShortcuts.value, action, shortcut);
  if (conflict) {
    $q.notify({
      type: 'warning',
      message: `该快捷键已用于“${EDITOR_SHORTCUT_LABELS[conflict]}”`,
    });
    return;
  }
  editorShortcuts.value = {...editorShortcuts.value, [action]: shortcut};
}

function notifyInvalidShortcut() {
  $q.notify({type: 'warning', message: '快捷键必须包含 Ctrl 或 Alt'});
}
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>
