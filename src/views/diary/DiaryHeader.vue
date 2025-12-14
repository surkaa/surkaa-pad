<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { showToast } from "../../utils"; // 假设utils路径一致，实际使用请根据目录调整

const props = defineProps<{
  mode: 'edit' | 'view';
  isNew: boolean;
  saveLoading: boolean;
  delLoading: boolean;
  undoStackLength: number;
  redoStackLength: number;
}>();

const emit = defineEmits<{
  (e: 'back'): void;
  (e: 'toggle-mode'): void;
  (e: 'undo'): void;
  (e: 'redo'): void;
  (e: 'save'): void;
  (e: 'delete'): void;
  (e: 'open-audio-drawer'): void;
  (e: 'upload-file', payload: { tagPrefix: string, file: File }): void;
}>();

const router = useRouter();
const showMediaMenu = ref(false);
const fileInputRef = ref<HTMLInputElement | null>(null);

function toggleMediaMenu() {
  showMediaMenu.value = !showMediaMenu.value;
}

function mediaSelected() {
  showMediaMenu.value = false;
}

// 触发图片选择
function triggerAddImage() {
  if (props.isNew) { return; } // 父组件已有校验，这里也可以保留或依赖父组件逻辑，但原逻辑是在触发时校验
  if (fileInputRef.value) {
    fileInputRef.value.accept = 'image/*';
    fileInputRef.value.click();
  }
  mediaSelected();
}

// 触发视频选择
function triggerAddVideo() {
  if (props.isNew) { return; }
  if (fileInputRef.value) {
    fileInputRef.value.accept = 'video/*';
    fileInputRef.value.click();
  }
  mediaSelected();
}

// 根据文件类型确定 Marker 前缀 (辅助函数)
function getTagPrefix(mimeType: string): 'IMG' | 'VID' | 'AUD' | null {
  if (mimeType.startsWith('image/')) return 'IMG';
  if (mimeType.startsWith('video/')) return 'VID';
  if (mimeType.startsWith('audio/')) return 'AUD';
  return null;
}

function handleMediaSelect(event: Event) {
  const input = event.target as HTMLInputElement;
  if (!input.files || input.files.length === 0) return;

  if (props.isNew) {
    showToast("请先保存一次日记再上传图片（需要生成日记ID）", 'info');
    input.value = "";
    return;
  }

  const file = input.files[0];
  const tagPrefix = getTagPrefix(file.type);

  if (!tagPrefix) {
    showToast("不支持的文件类型: " + file.type, 'error');
    input.value = "";
    return;
  }

  // 将文件传递给父组件处理上传逻辑
  emit('upload-file', { tagPrefix, file });
  input.value = "";
}

function handleBack() {
  // 原逻辑直接调用 router.back()，但为了统一路由守卫触发，还是调用 router.back()
  // 也可以 emit 'back' 让父组件决定，但原代码在 HTML 中直接使用了 router.back()
  router.back();
}
</script>

<template>
  <section id="diary-detail-header">
    <button
        id="diary-detail-header-back-btn"
        @click="handleBack"
        class="btn-icon"
        aria-label="返回"
    >
      <svg viewBox="0 0 24 24" width="20" height="20">
        <path d="M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z"/>
      </svg>
    </button>

    <div class="header-controls">
      <button
          class="btn-icon toggle-mode"
          @click="emit('toggle-mode')"
          :class="{ 'active': mode === 'edit' }"
          :aria-label="mode === 'edit' ? '切换到查看模式' : '切换到编辑模式'"
      >
        <svg v-if="mode === 'edit'" viewBox="0 0 24 24" width="20" height="20">
          <path d="M12 4.5C7 4.5 2.73 7.61 1 12c1.73 4.39 6 7.5 11 7.5s9.27-3.11 11-7.5c-1.73-4.39-6-7.5-11-7.5zM12 17c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5zm0-8c-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3-1.34-3-3-3z"/>
        </svg>
        <svg v-else viewBox="0 0 24 24" width="20" height="20">
          <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"/>
        </svg>
      </button>

      <div class="history-controls">
        <button
            class="btn-icon undo-btn"
            @click="emit('undo')"
            :disabled="undoStackLength === 0"
            :class="{ 'disabled': undoStackLength === 0 }"
            aria-label="撤销"
        >
          <svg viewBox="0 0 24 24" width="20" height="20">
            <path d="M12.5 8c-2.65 0-5.05.99-6.9 2.6L2 7v9h9l-3.62-3.62c1.39-1.16 3.16-1.88 5.12-1.88 3.54 0 6.55 2.31 7.6 5.5l2.37-.78C21.08 11.03 17.15 8 12.5 8z"/>
          </svg>
        </button>
        <button
            class="btn-icon redo-btn"
            @click="emit('redo')"
            :disabled="redoStackLength === 0"
            :class="{ 'disabled': redoStackLength === 0 }"
            aria-label="重做"
        >
          <svg viewBox="0 0 24 24" width="20" height="20">
            <path d="M18.4 10.6C16.55 8.99 14.15 8 11.5 8c-4.65 0-8.58 3.03-9.96 7.22L3.9 16c1.05-3.19 4.05-5.5 7.6-5.5 1.95 0 3.73.72 5.12 1.88L13.5 16H22V7l-3.6 3.6z"/>
          </svg>
        </button>
      </div>

      <div id="media-menu-container" v-click-outside="() => showMediaMenu = false">
        <button
            class="btn-icon media-menu-btn"
            @click="toggleMediaMenu"
            :disabled="saveLoading || isNew"
            :class="{ 'disabled': saveLoading || isNew }"
            aria-label="添加媒体"
        >
          <svg viewBox="0 0 24 24" width="20" height="20">
            <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/>
          </svg>
        </button>

        <transition name="media-menu">
          <div v-if="showMediaMenu" id="media-menu-dropdown">
            <button @click="triggerAddImage" class="media-option">
              <svg viewBox="0 0 24 24" width="16" height="16">
                <path d="M21 19V5c0-1.1-.9-2-2-2H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2zM8.5 13.5l2.5 3.01L14.5 12l4.5 6H5l3.5-4.5z"/>
              </svg>
              <span>图片</span>
            </button>
            <button @click="triggerAddVideo" class="media-option">
              <svg viewBox="0 0 24 24" width="16" height="16">
                <path d="M17 10.5V7c0-.55-.45-1-1-1H4c-.55 0-1 .45-1 1v10c0 .55.45 1 1 1h12c.55 0 1-.45 1-1v-3.5l4 4v-11l-4 4z"/>
              </svg>
              <span>视频</span>
            </button>
            <button @click="emit('open-audio-drawer'); showMediaMenu = false" class="media-option">
              <svg viewBox="0 0 24 24" width="16" height="16">
                <path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z"/>
                <path d="M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/>
              </svg>
              <span>录音</span>
            </button>
          </div>
        </transition>
      </div>
    </div>

    <div class="header-actions">
      <input
          type="file"
          ref="fileInputRef"
          style="display: none"
          accept="image/*,video/*"
          @change="handleMediaSelect"
          multiple
      />

      <button
          id="diary-detail-header-save-btn"
          @click="emit('save')"
          :disabled="saveLoading"
          :class="{
            'btn-primary': isNew,
            'btn-secondary': !isNew,
            'loading': saveLoading
          }"
          aria-label="保存日记"
      >
        <span v-if="saveLoading" class="loading-spinner"></span>
        <span v-else class="btn-text">
            {{ isNew ? '创建' : '保存' }}
          </span>
      </button>

      <button
          v-if="!isNew"
          id="diary-detail-header-delete-btn"
          @click="emit('delete')"
          :disabled="delLoading"
          :class="{ 'loading': delLoading }"
          aria-label="删除日记"
      >
        <span v-if="delLoading" class="loading-spinner"></span>
        <span v-else class="btn-text">删除</span>
      </button>
    </div>
  </section>
</template>

<style scoped lang="scss">
/* 从原 Diary.vue 提取的 Header 样式 */
#diary-detail-header {
  height: 60px;
  min-height: 60px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 20px;
  background-color: var(--pad-bg-color-100);
  border-bottom: 1px solid var(--pad-border-color-200);
  gap: 16px;
  flex-shrink: 0;
  box-shadow: var(--pad-shadow-sm);

  // 返回按钮
  #diary-detail-header-back-btn {
    background: transparent;
    border: none;
    padding: 8px;
    cursor: pointer;
    border-radius: var(--pad-radius-full);
    color: var(--pad-text-color-300);
    transition: all var(--pad-transition-fast);
    display: flex;
    align-items: center;
    justify-content: center;

    svg {
      fill: currentColor;
    }

    &:hover {
      background-color: var(--pad-bg-color-300);
      color: var(--pad-text-color-100);
    }

    &:active {
      transform: scale(0.95);
    }

    @media (max-width: 768px) {
      margin-right: 0;
    }
  }

  // 头部控制区域
  .header-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;

    .btn-icon {
      background: transparent;
      border: none;
      padding: 8px;
      cursor: pointer;
      border-radius: var(--pad-radius-full);
      color: var(--pad-text-color-300);
      transition: all var(--pad-transition-fast);
      display: flex;
      align-items: center;
      justify-content: center;

      svg {
        fill: currentColor;
      }

      &:hover:not(.disabled) {
        background-color: var(--pad-bg-color-300);
        color: var(--pad-text-color-100);
      }

      &:active:not(.disabled) {
        transform: scale(0.95);
      }

      &.active {
        background-color: var(--pad-primary-color);
        color: var(--pad-text-color-light);

        &:hover {
          background-color: var(--pad-primary-dark);
        }
      }

      &.disabled {
        opacity: 0.4;
        cursor: not-allowed;

        &:hover {
          background-color: transparent;
          color: var(--pad-text-color-300);
        }
      }
    }

    .history-controls {
      display: flex;
      gap: 4px;
      position: relative;

      &::before {
        content: '';
        position: absolute;
        left: -6px;
        top: 50%;
        transform: translateY(-50%);
        height: 20px;
        width: 1px;
        background-color: var(--pad-border-color-100);
      }
    }
  }

  // 媒体菜单容器
  #media-menu-container {
    position: relative;
    display: inline-block;

    .media-menu-btn {
      &.disabled {
        opacity: 0.4;
        cursor: not-allowed;

        &:hover {
          background-color: transparent;
          color: var(--pad-text-color-300);
        }
      }
    }

    #media-menu-dropdown {
      position: absolute;
      top: calc(100% + 8px);
      right: 0;
      z-index: 100;
      background-color: var(--pad-bg-color-100);
      border: 1px solid var(--pad-border-color-200);
      box-shadow: var(--pad-shadow-lg);
      min-width: 140px;
      border-radius: var(--pad-radius-lg);
      padding: 8px;
      animation: slideDown 0.2s ease-out;

      &::before {
        content: '';
        position: absolute;
        top: -6px;
        right: 16px;
        width: 12px;
        height: 12px;
        background-color: var(--pad-bg-color-100);
        border-left: 1px solid var(--pad-border-color-200);
        border-top: 1px solid var(--pad-border-color-200);
        transform: rotate(45deg);
      }

      .media-option {
        display: flex;
        align-items: center;
        gap: 12px;
        width: 100%;
        padding: 10px 12px;
        border: none;
        background: none;
        text-align: left;
        cursor: pointer;
        font-size: 14px;
        color: var(--pad-text-color-200);
        border-radius: var(--pad-radius-md);
        transition: all var(--pad-transition-fast);

        svg {
          fill: currentColor;
          opacity: 0.8;
        }

        &:hover {
          background-color: var(--pad-bg-color-200);
          color: var(--pad-primary-color);

          svg {
            opacity: 1;
          }
        }

        &:active {
          transform: scale(0.98);
        }
      }
    }

    .media-menu-enter-active,
    .media-menu-leave-active {
      transition: opacity var(--pad-transition-fast), transform var(--pad-transition-fast);
    }

    .media-menu-enter-from,
    .media-menu-leave-to {
      opacity: 0;
      transform: translateY(-10px);
    }
  }

  // 头部操作区域
  .header-actions {
    display: flex;
    align-items: center;
    gap: 12px;

    button {
      padding: 8px 20px;
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      border-radius: var(--pad-radius-lg);
      border: none;
      transition: all var(--pad-transition-base);
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
      min-height: 36px;

      &:disabled {
        opacity: 0.6;
        cursor: not-allowed;
      }

      &.loading {
        opacity: 0.8;
      }

      .loading-spinner {
        width: 16px;
        height: 16px;
        border: 2px solid rgba(255, 255, 255, 0.3);
        border-top-color: white;
        border-radius: 50%;
        animation: spin 1s linear infinite;
      }

      .btn-text {
        font-weight: 500;
        letter-spacing: 0.3px;
      }
    }

    #diary-detail-header-save-btn {
      min-width: 60px;

      &.btn-primary {
        background: var(--pad-primary-gradient);
        color: var(--pad-text-color-light);

        &:hover:not(:disabled) {
          box-shadow: var(--pad-shadow-md);
          transform: translateY(-1px);
        }

        &:active:not(:disabled) {
          transform: translateY(0);
        }
      }

      &.btn-secondary {
        background-color: var(--pad-bg-color-300);
        color: var(--pad-text-color-200);
        border: 1px solid var(--pad-border-color-200);

        &:hover:not(:disabled) {
          background-color: var(--pad-bg-color-400);
          color: var(--pad-text-color-100);
        }
      }
    }

    #diary-detail-header-delete-btn {
      background-color: transparent;
      color: var(--pad-danger-color);
      border: 1px solid var(--pad-danger-light);

      &:hover:not(:disabled) {
        background-color: var(--pad-danger-color);
        color: var(--pad-text-color-light);
      }

      &.loading .loading-spinner {
        border-top-color: var(--pad-danger-color);
      }
    }
  }
}

// 动画
@keyframes slideDown {
  from {
    opacity: 0;
    transform: translateY(-8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

// 响应式设计
@media (max-width: 768px) {
  #diary-detail-header {
    height: 56px;
    gap: 12px;

    .header-actions {
      gap: 8px;

      button {
        padding: 8px 16px;
        font-size: 13px;
      }
    }
  }
}

@media (max-width: 480px) {
  #diary-detail-header {
    padding: 0 12px;

    #diary-detail-header-back-btn {
      padding: 6px;
    }

    .header-controls .btn-icon {
      padding: 6px;
    }

    .header-actions {
      gap: 3px;

      button {
        padding: 6px 12px;
        min-width: 60px;
      }
    }
  }
}
</style>
