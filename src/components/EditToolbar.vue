<script setup lang="ts">

import {computed} from 'vue';
import type { Editor } from '@tiptap/vue-3'
import { platform } from "@tauri-apps/plugin-os";
import {
  formatEditorShortcut,
  type EditorShortcutAction,
  type EditorShortcutConfig,
} from "../utils/editorShortcuts.ts";
import {
  EDITOR_TOOLBAR_LABELS,
  normalizeEditorToolbarOrder,
  type EditorToolbarAction,
} from '../utils/editorToolbar';

const {
  view,
  panelOpen,
  editor,
  shortcuts,
  toolbarOrder,
} = defineProps<{
  view: boolean,
  panelOpen: boolean,
  editor?: Editor | null,
  shortcuts?: EditorShortcutConfig,
  toolbarOrder?: EditorToolbarAction[],
}>();

const currentPlatform = platform();
const isAndroid = currentPlatform === 'android';
const isWindows = currentPlatform === 'windows';
const orderedToolbarActions = computed(() => normalizeEditorToolbarOrder(toolbarOrder));

function isToolbarActionActive(action: EditorToolbarAction): boolean {
  if (!editor) return false;
  switch (action) {
    case 'bold': return editor.isActive('bold');
    case 'underline': return editor.isActive('underline');
    case 'strike': return editor.isActive('strike');
    case 'heading1': return editor.isActive('heading', {level: 1});
    case 'heading2': return editor.isActive('heading', {level: 2});
    case 'heading3': return editor.isActive('heading', {level: 3});
    case 'taskList': return editor.isActive('taskList');
  }
}

function runToolbarAction(action: EditorToolbarAction) {
  if (!editor) return;
  switch (action) {
    case 'bold': editor.chain().focus().toggleBold().run(); break;
    case 'underline': editor.chain().focus().toggleUnderline().run(); break;
    case 'strike': editor.chain().focus().toggleStrike().run(); break;
    case 'heading1': editor.chain().focus().toggleHeading({level: 1}).run(); break;
    case 'heading2': editor.chain().focus().toggleHeading({level: 2}).run(); break;
    case 'heading3': editor.chain().focus().toggleHeading({level: 3}).run(); break;
    case 'taskList': editor.chain().focus().toggleTaskList().run(); break;
  }
}

function toolbarActionText(action: EditorToolbarAction): string {
  return {
    bold: 'B',
    underline: 'U',
    strike: 'S',
    heading1: 'H1',
    heading2: 'H2',
    heading3: 'H3',
    taskList: '',
  }[action];
}

function attachmentActionTitle(label: string, action: EditorShortcutAction) {
  const shortcut = shortcuts?.[action];
  if (!isWindows || !shortcut) return label;
  return `${label}（${formatEditorShortcut(shortcut)}）`;
}

const emit = defineEmits([
  'undo', 'redo',
  'additionalAction',
  'insertPhoto', 'takePhoto',
  'insertAudio', 'audioRecording',
  'insertVideo', 'takeVideo',
  'insertFile'
]);
</script>

<template>
  <transition name="slide-up">
    <div v-show="view" class="edit-toolbar">

      <div class="toolbar-header">
        <div class="toolbar-scroll">
          <template v-if="editor">
            <button
              v-for="action in orderedToolbarActions"
              :key="action"
              class="tool-btn"
              :class="{ 'is-active': isToolbarActionActive(action) }"
              :title="EDITOR_TOOLBAR_LABELS[action]"
              :aria-label="EDITOR_TOOLBAR_LABELS[action]"
              @click.stop="runToolbarAction(action)"
            >
              <q-icon v-if="action === 'taskList'" name="checklist" size="20px"/>
              <b v-else-if="action === 'bold'">{{ toolbarActionText(action) }}</b>
              <u v-else-if="action === 'underline'">{{ toolbarActionText(action) }}</u>
              <s v-else-if="action === 'strike'">{{ toolbarActionText(action) }}</s>
              <template v-else>{{ toolbarActionText(action) }}</template>
            </button>
            <div class="divider"></div>
          </template>
          <button class="tool-btn" @click.stop="emit('undo')">↺</button>
          <button class="tool-btn" @click.stop="emit('redo')">↻</button>
        </div>
        <div class="divider"></div>

        <div class="fixed-actions">
          <button class="tool-btn add-btn"
                  @click.stop="emit('additionalAction')"
                  :class="{ 'is-open': panelOpen }">
            +
          </button>
        </div>
      </div>

      <transition name="panel-expand">
        <div v-show="panelOpen" class="more-panel">
          <div>
            <q-btn flat stack color="grey-8" class="panel-item-btn"
                   :title="attachmentActionTitle('照片', 'insertPhoto')" @click="emit('insertPhoto')">
              <q-icon name="image" size="28px" class="q-mb-xs"/>
              <span class="text-caption">照片</span>
            </q-btn>
            <q-btn v-if="isAndroid" flat stack color="grey-8" class="panel-item-btn" @click="emit('takePhoto')">
              <q-icon name="camera" size="28px" class="q-mb-xs"/>
              <span class="text-caption">拍摄</span>
            </q-btn>
            <q-btn flat stack color="grey-8" class="panel-item-btn"
                   :title="attachmentActionTitle('音频', 'insertAudio')" @click="emit('insertAudio')">
              <q-icon name="audiotrack" size="28px" class="q-mb-xs"/>
              <span class="text-caption">音频</span>
            </q-btn>
            <q-btn flat stack color="grey-8" class="panel-item-btn"
                   :title="attachmentActionTitle('录音', 'audioRecording')" @click="emit('audioRecording')">
              <q-icon name="mic" size="28px" class="q-mb-xs"/>
              <span class="text-caption">录音</span>
            </q-btn>
            <q-btn flat stack color="grey-8" class="panel-item-btn"
                   :title="attachmentActionTitle('视频', 'insertVideo')" @click="emit('insertVideo')">
              <q-icon name="video_library" size="28px" class="q-mb-xs"/>
              <span class="text-caption">视频</span>
            </q-btn>
            <q-btn flat stack color="grey-8" class="panel-item-btn"
                   :title="attachmentActionTitle('文件', 'insertFile')" @click="emit('insertFile')">
              <q-icon name="attach_file" size="28px" class="q-mb-xs"/>
              <span class="text-caption">文件</span>
            </q-btn>
          </div>
        </div>
      </transition>
    </div>
  </transition>
</template>

<style scoped lang="scss">
.edit-toolbar {
  width: 100%;
  background-color: var(--pad-bg-color-300);
  border-top: 1px solid var(--pad-border-color);
  box-shadow: 0 -2px 10px var(--pad-shadow-color-200);

  display: flex;
  align-items: center;
  flex-direction: column;

  .toolbar-header {
    display: flex;
    align-items: center;
    width: 100%;
    height: 50px;
    position: relative;
  }

  .toolbar-scroll {
    flex: 1;
    display: flex;
    align-items: center;
    overflow-x: auto; /* 允许横向滚动 */
    padding: 0 10px;
    width: 100%;
    height: 50px;
    flex-shrink: 0; /* 防止被挤压 */
    gap: 8px;
    /* 隐藏滚动条但保留滚动功能 */
    scrollbar-width: none;

    &::-webkit-scrollbar {
      display: none;
    }
  }

  .fixed-actions {
    display: flex;
    align-items: center;
    padding: 0 10px;
    background-color: var(--pad-bg-color-300);
    box-shadow: -10px 0 10px -5px var(--pad-bg-color-300);
    z-index: 10;
    height: 100%;
  }

  .tool-btn {
    flex-shrink: 0;
    height: 36px;
    min-width: 36px;
    padding: 0 10px;
    border: none;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s ease;

    background: var(--pad-bg-color-300);
    color: var(--pad-text-color);

    &.is-active {
      background: var(--pad-primary-color);
      color: var(--pad-on-primary-color);
    }

    &:disabled {
      opacity: 0.4;
      cursor: not-allowed;
      background: transparent;
      color: var(--pad-text-color);
    }

    &.add-btn {
      font-size: 20px;
      transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1), background-color 0.2s;

      &.is-open {
        transform: rotate(45deg);
        background-color: var(--pad-bg-color-400);
      }
    }
  }

  .divider {
    width: 1px;
    height: 20px;
    background: var(--pad-shadow-color-100);
    margin: 0 4px;
    flex-shrink: 0;
  }

  .more-panel {
    width: 100%;
    background-color: var(--pad-bg-color-200);
    overflow: hidden;

    .panel-item-btn {
      :deep(.q-icon) {
        color: var(--pad-text-color);
      }

      .text-caption {
        color: var(--pad-text-color);
        font-size: 12px;
        line-height: 1.2;
      }
    }
  }
}

/* 整体 Toolbar 滑出动画 */
.slide-up-enter-active,
.slide-up-leave-active {
  transition: transform 0.2s ease-out, opacity 0.2s;
}

.slide-up-enter-from,
.slide-up-leave-to {
  transform: translateY(100%);
  opacity: 0;
}

/* 面板展开动画 */
.panel-expand-enter-active,
.panel-expand-leave-active {
  transition: all 0.3s cubic-bezier(0.25, 0.8, 0.5, 1);
  max-height: 300px;
  opacity: 1;
}

.panel-expand-enter-from,
.panel-expand-leave-to {
  max-height: 0;
  opacity: 0;
  padding-top: 0 !important;
  padding-bottom: 0 !important;
}

.more-panel {
  padding: 20px 10px;
}
</style>
