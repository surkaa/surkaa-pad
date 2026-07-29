<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">快捷键</div>
    <q-list bordered class="pad-card shortcut-groups">
      <q-expansion-item group="shortcut-page" expand-separator>
        <template #header>
          <q-item-section avatar class="settings-icon-section">
            <q-icon name="view_list"/>
          </q-item-section>
          <q-item-section>
            <q-item-label class="label-text text-weight-medium">日记列表</q-item-label>
            <q-item-label caption class="desc-text">{{ DIARY_LIST_SHORTCUT_ACTIONS.length }} 个操作</q-item-label>
          </q-item-section>
        </template>

        <q-item
          v-for="action in DIARY_LIST_SHORTCUT_ACTIONS"
          :key="action"
          class="settings-item shortcut-settings-item shortcut-action-item"
        >
          <q-item-section avatar class="settings-icon-section">
            <q-icon :name="diaryListShortcutIcons[action]"/>
          </q-item-section>
          <q-item-section>
            <q-item-label class="label-text text-weight-medium">
              {{ DIARY_LIST_SHORTCUT_LABELS[action] }}
            </q-item-label>
          </q-item-section>
          <q-item-section side>
            <ShortcutRecorder
              :model-value="diaryListShortcuts[action]"
              :label="DIARY_LIST_SHORTCUT_LABELS[action]"
              @update:model-value="shortcut => updateDiaryListShortcut(action, shortcut)"
              @invalid="notifyInvalidShortcut"
            />
          </q-item-section>
        </q-item>
      </q-expansion-item>

      <q-expansion-item group="shortcut-page">
        <template #header>
          <q-item-section avatar class="settings-icon-section">
            <q-icon name="edit_note"/>
          </q-item-section>
          <q-item-section>
            <q-item-label class="label-text text-weight-medium">日记编辑</q-item-label>
            <q-item-label caption class="desc-text">{{ EDITOR_SHORTCUT_ACTIONS.length }} 个操作</q-item-label>
          </q-item-section>
        </template>

        <q-item
          v-for="action in EDITOR_SHORTCUT_ACTIONS"
          :key="action"
          class="settings-item shortcut-settings-item shortcut-action-item"
        >
          <q-item-section avatar class="settings-icon-section">
            <q-icon :name="editorShortcutIcons[action]"/>
          </q-item-section>
          <q-item-section>
            <q-item-label class="label-text text-weight-medium">
              {{ EDITOR_SHORTCUT_LABELS[action] }}
            </q-item-label>
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
      </q-expansion-item>
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
import {
  DIARY_LIST_SHORTCUT_ACTIONS,
  DIARY_LIST_SHORTCUT_LABELS,
  findDiaryListShortcutConflict,
  type DiaryListShortcutAction,
} from '../../utils/diaryListShortcuts';

const $q = useQuasar();
const configStore = useConfigStore();
const editorShortcuts = configStore.useTauriConfig('windows_editor_shortcuts');
const diaryListShortcuts = configStore.useTauriConfig('windows_diary_list_shortcuts');
const editorShortcutIcons: Record<EditorShortcutAction, string> = {
  insertPhoto: 'image',
  insertAudio: 'audiotrack',
  audioRecording: 'mic',
  insertVideo: 'video_library',
  insertFile: 'attach_file',
};
const diaryListShortcutIcons: Record<DiaryListShortcutAction, string> = {
  search: 'search',
  settings: 'settings',
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

function updateDiaryListShortcut(action: DiaryListShortcutAction, shortcut: string) {
  const conflict = findDiaryListShortcutConflict(diaryListShortcuts.value, action, shortcut);
  if (conflict) {
    $q.notify({
      type: 'warning',
      message: `该快捷键已用于“${DIARY_LIST_SHORTCUT_LABELS[conflict]}”`,
    });
    return;
  }
  diaryListShortcuts.value = {...diaryListShortcuts.value, [action]: shortcut};
}

function notifyInvalidShortcut() {
  $q.notify({type: 'warning', message: '快捷键必须包含 Ctrl 或 Alt'});
}
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>

<style scoped lang="scss">
.shortcut-groups {
  :deep(.q-expansion-item__container > .q-item) {
    min-height: 66px;
    padding: 10px 16px;
  }

  :deep(.q-expansion-item__toggle-icon) {
    color: var(--pad-text-color-400);
  }
}

.shortcut-action-item {
  background: color-mix(in srgb, var(--pad-bg-color-100) 36%, transparent);
  border-top: 1px solid var(--pad-border-color-100);
}

@media (max-width: 600px) {
  .shortcut-groups :deep(.q-expansion-item__container > .q-item) {
    padding-right: 12px;
    padding-left: 12px;
  }
}
</style>
