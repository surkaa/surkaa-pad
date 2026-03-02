<script setup lang="ts">

import {platform} from "@tauri-apps/plugin-os";

const {
  view,
  panelOpen,
  undo,
  redo,
} = defineProps<{
  view: boolean,
  panelOpen: boolean,
  undo: boolean,
  redo: boolean,
}>();

const isAndroid = platform() === 'android';

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
          <button class="tool-btn" @click.stop="emit('undo')" :disabled="!undo">↺</button>
          <button class="tool-btn" @click.stop="emit('redo')" :disabled="!redo">↻</button>
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
          <div class="row q-col-gutter-md">
            <div class="col-3 flex flex-center">
              <q-btn flat stack color="grey-8" class="panel-item-btn" @click="emit('insertPhoto')">
                <q-icon name="image" size="28px" class="q-mb-xs"/>
                <span class="text-caption">照片</span>
              </q-btn>
            </div>
            <div class="col-3 flex flex-center" v-if="isAndroid">
              <q-btn flat stack color="grey-8" class="panel-item-btn" @click="emit('takePhoto')">
                <q-icon name="camera" size="28px" class="q-mb-xs"/>
                <span class="text-caption">拍摄</span>
              </q-btn>
            </div>
            <div class="col-3 flex flex-center">
              <q-btn flat stack color="grey-8" class="panel-item-btn" @click="emit('insertAudio')">
                <q-icon name="audiotrack" size="28px" class="q-mb-xs"/>
                <span class="text-caption">音频</span>
              </q-btn>
            </div>
            <div class="col-3 flex flex-center" v-if="isAndroid">
              <q-btn flat stack color="grey-8" class="panel-item-btn" @click="emit('audioRecording')">
                <q-icon name="mic" size="28px" class="q-mb-xs"/>
                <span class="text-caption">录音</span>
              </q-btn>
            </div>
            <div class="col-3 flex flex-center">
              <q-btn flat stack color="grey-8" class="panel-item-btn" @click="emit('insertVideo')">
                <q-icon name="video_library" size="28px" class="q-mb-xs"/>
                <span class="text-caption">视频</span>
              </q-btn>
            </div>
            <div class="col-3 flex flex-center">
              <q-btn flat stack color="grey-8" class="panel-item-btn" @click="emit('insertFile')">
                <q-icon name="attach_file" size="28px" class="q-mb-xs"/>
                <span class="text-caption">文件</span>
              </q-btn>
            </div>
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
      width: 100%;

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
