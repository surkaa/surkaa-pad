<script setup lang="ts">
import {computed, onMounted, ref, watch} from "vue";
import {DiaryManifest} from "../types";
import {invoke} from "@tauri-apps/api/core";
import {onBeforeRouteLeave, useRouter} from "vue-router";
import {formatTimestamp, getCurEmoji} from "../utils";
import {showToast} from "../utils";
import RichTextEditor from "../components/RichTextEditor.vue";
import {saveAttachment} from "../utils";
import CaptureAudioDrawer from "../components/CaptureAudioDrawer.vue";

const router = useRouter();

// 默认值用于新建日记
const DEFAULT_DIARY: DiaryManifest = {
  id: "",
  content: "",
  created: Date.now(),
  updated: Date.now(),
  algorithm: "AES-256-GCM", // 默认加密算法
  attachments: []
} as const;

const diary = ref<DiaryManifest>(DEFAULT_DIARY);
const renderLoading = ref(false);
const saveLoading = ref(false);
const delLoading = ref(false);
const uploadLoading = ref(false);
const isNew = computed(() => !diary.value.id); // 判断是否为新建日记
const mode = ref<'edit' | 'view'>('view');
const maxUndoStackSize = 5;
const undoStack = ref<string[]>([]);
const redoStack = ref<string[]>([]);
const undoOrRedoInProgress = ref(false);
const renderMsg = ref('');
const isDelBack = ref(false);
const showAudioDrawer = ref(false);

// 文件输入框引用
const fileInputRef = ref<HTMLInputElement | null>(null);
const showMediaMenu = ref(false);
const statusMsg = computed(() => {
  if (uploadLoading.value) {
    return "上传附件中...";
  }
  if (saveLoading.value) {
    return "保存日记中...";
  }
  if (renderLoading.value) {
    return "加载日记中...";
  }
  if (delLoading.value) {
    return "删除日记中...";
  }
  return mode.value === 'edit' ? '编辑模式' : renderMsg.value;
});
const cursorPosition = ref(0);
const lastSavedContent = ref("");

function toggleMediaMenu() {
  // 切换菜单显示状态
  showMediaMenu.value = !showMediaMenu.value;
}

// 媒体选择后的通用处理函数 (用于关闭菜单)
function mediaSelected() {
  showMediaMenu.value = false;
}

// 触发图片选择
function triggerAddImage() {
  if (isNew.value) { /* 提醒逻辑 */
    return;
  }
  if (fileInputRef.value) {
    fileInputRef.value.accept = 'image/*';
    fileInputRef.value.click();
  }
  mediaSelected(); // 关闭菜单
}

// 触发视频选择
function triggerAddVideo() {
  if (isNew.value) { /* 提醒逻辑 */
    return;
  }
  if (fileInputRef.value) {
    fileInputRef.value.accept = 'video/*';
    fileInputRef.value.click();
  }
  mediaSelected(); // 关闭菜单
}

const contentLen = computed(() => {
  // 去除掉<<tag:filename>>标记、换行、空格后的纯文本长度
  if (!diary.value.content) return 0;
  const textOnly = diary.value.content.replace(/<<[A-Z]{3}:[^>]+>>/g, '').replace(/\s+/g, '');
  return textOnly.length;
});

function toggleMode() {
  mode.value = (mode.value === 'edit' ? 'view' : 'edit');
}

onBeforeRouteLeave((to, from, next) => {
  console.log('准备离开日记详情页', {to, from});
  if (isDelBack.value) {
    // 如果是删除后返回，直接放行
    next();
    return;
  }
  // 如果是新建日记且内容为空，直接返回
  if (isNew.value && (!diary.value.content || diary.value.content.length === 0)) {
    next();
    return;
  }
  // 如果是新建日记且内容不为空，提示保存
  if (isNew.value && diary.value.content && diary.value.content.length > 0) {
    const confirmSave = confirm("日记尚未保存，要不先保存再返回？");
    if (confirmSave) {
      saveDiary().then(next);
      return;
    } else {
      // 放弃保存，直接返回
      console.log("放弃保存日记，直接返回");
      next();
      return;
    }
  }
  // 如果内容有变更，提示保存
  if (diary.value.content !== lastSavedContent.value) {
    const confirmSave = confirm("日记内容有变更，要不先保存再返回？");
    if (confirmSave) {
      saveDiary().then(next);
      return;
    } else {
      // 放弃保存，直接返回
      console.log("放弃保存日记，直接返回");
      next();
    }
  }
  // 否则直接放行
  next();
});

// 保存或者更新日记
async function saveDiary(afterAddAttachment = false) {
  saveLoading.value = true;
  // if (!editorRef.value) return;
  //
  const currentMedias = Array.from(document.querySelectorAll('.media-item'))
      .map(el => (el as HTMLElement).dataset.filename)
      .filter(fn => fn) as string[];

  // 从 DOM 解析回纯文本 + 标记
  // diary.value.content = parseHtmlToText(editorRef.value);

  if (!diary.value.content || diary.value.content.length === 0) {
    showToast("日记内容不能为空", 'warning');
    saveLoading.value = false;
    return;
  }

  try {

    // 找出原有附件列表中，现在已经不存在于编辑器里的文件 如果是新增附件操作后保存，则跳过此步骤 要不然会误删刚上传的附件
    if (!isNew.value && diary.value.attachments && !afterAddAttachment) {
      const filesToDelete = diary.value.attachments.filter(att => {
        // 如果附件在当前编辑器里找不到，说明被删了
        return !currentMedias.includes(att.filename);
      });

      if (filesToDelete.length > 0) {
        console.log("检测到孤儿附件，准备清理:", filesToDelete);

        // 并行调用删除接口 必须删完再保存 保持数据一致性
        await Promise.all(filesToDelete.map(att =>
            invoke("delete_attachment", {
              uuid: diary.value.id,
              filename: att.filename
            })
        ));
      }
    }

    if (isNew.value) {
      // 新建日记
      console.log("新建日记", diary.value);
      const d = await invoke<DiaryManifest>("save_diary", {
        content: diary.value.content
      });
      diary.value = d;
      lastSavedContent.value = d.content;
      console.log("日记保存成功, Diary: ", d);
      showToast("日记保存成功", 'success');
    } else {
      // 更新日记
      console.log("更新日记, Old Diary: ", diary.value);
      const d = await invoke<DiaryManifest>("update_diary_content_only", {
        uuid: diary.value.id,
        newContent: diary.value.content
      });
      diary.value = d;
      lastSavedContent.value = d.content;
      console.log("日记更新成功, Diary: ", d);
    }
  } catch (e) {
    console.error("保存日记失败", e);
    showToast('保存日记失败: ' + e, 'error');
  } finally {
    saveLoading.value = false;
  }
}

// 删除当前日记
async function deleteDiary() {
  if (isNew.value) {
    const confirmAbandon = confirm("当前日记未保存, 确认放弃并返回吗?");
    if (confirmAbandon) router.back();
    return;
  }

  const confirmDelete = confirm("⚠️ 确认永久删除这篇日记吗?");
  if (!confirmDelete) return;

  delLoading.value = true;
  try {
    await invoke("delete_diary", {uuid: diary.value.id});
    console.log("日记删除成功");
    isDelBack.value = true;
    router.back();
    showToast('日记删除成功', 'success');
  } catch (e) {
    console.error("删除日记失败", e);
    showToast("删除日记失败: " + e, 'error');
  } finally {
    delLoading.value = false;
  }
}

// 根据文件类型确定 Marker 前缀
function getTagPrefix(mimeType: string): 'IMG' | 'VID' | 'AUD' | null {
  if (mimeType.startsWith('image/')) return 'IMG';
  if (mimeType.startsWith('video/')) return 'VID';
  if (mimeType.startsWith('audio/')) return 'AUD';
  return null;
}

function handleMediaSelect(event: Event) {
  const input = event.target as HTMLInputElement;
  if (!input.files || input.files.length === 0) return;

  if (isNew.value) {
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

  console.log("选择的图片文件: ", file);

  uploadAttachment(tagPrefix, file.type, file.stream()).then(() => {
    input.value = "";
  });
}

function recordedAudio(minetype: string, stream: ReadableStream<Uint8Array>) {
  uploadAttachment('AUD', minetype, stream);
}

// 处理图片选择与上传
async function uploadAttachment(tagPrefix: string, minetype: string, stream: ReadableStream<Uint8Array>) {
  try {
    uploadLoading.value = true;
    // 插入前先保存当前日记内容，确保最新状态，避免删掉的东西又被加回去
    await saveDiary();

    // 调用后端上传
    const updatedManifest = await saveAttachment(diary.value.id, minetype, stream);

    // 找出新增加的文件名
    // 比较新旧 attachments 列表，找到多出来的那个
    const oldFiles = diary.value.attachments.map(a => a.filename);
    const newFile = updatedManifest.attachments.find(a => !oldFiles.includes(a.filename));

    if (!newFile) {
      // throw new Error("无法获取新上传的文件名");
      showToast("上传成功，但无法获取新上传的文件名", 'success');
      return;
    }

    // 更新本地数据
    diary.value = updatedManifest;

    // 在光标位置插入图片
    const marker = `\n<<${tagPrefix}:${newFile.filename}>>\n`;
    const content = diary.value.content || "";
    const before = content.slice(0, cursorPosition.value);
    const after = content.slice(cursorPosition.value);
    diary.value.content = before + marker + after;
    console.log('插入附件标记: ', marker);

    // 自动更新保存日记
    await saveDiary(true);
  } catch (e) {
    console.error("上传图片失败", e);
    showToast("上传图片失败: " + e, 'error');
  } finally {
    uploadLoading.value = false;
  }
}

function updateCursor(position: number) {
  cursorPosition.value = position;
  console.log('当前光标位置：', position);
}

function undo() {
  if (undoStack.value.length === 0) return;
  const lastState = undoStack.value.pop()!;
  // 推入重做栈
  redoStack.value.push(diary.value.content || "");
  undoOrRedoInProgress.value = true;
  // 恢复内容
  diary.value.content = lastState;
}

function redo() {
  if (redoStack.value.length === 0) return;
  const nextState = redoStack.value.pop()!;
  // 推入撤销栈
  undoStack.value.push(diary.value.content || "");
  undoOrRedoInProgress.value = true;
  // 恢复内容
  diary.value.content = nextState;
}

function openPreviewMedia(eid: string, minetype: string) {
  console.log('打开媒体预览: ', eid, minetype);
  router.push({
    name: 'PreviewMedia',
    state: {
      eid, minetype
    }
  });
}

onMounted(async () => {
  if (history.state.diary) {
    diary.value = history.state.diary;
    lastSavedContent.value = diary.value.content;
  }
  // 新增模式时默认为编辑模式
  mode.value = isNew.value ? 'edit' : 'view';
  watch(() => diary.value.content, (value, oldValue, _) => {
    console.log("日记内容变更检测: ", {oldLen: oldValue?.length || 0, newLen: value?.length || 0});
    if (undoOrRedoInProgress.value) {
      // 如果是撤销或重做引起的变更，不记录历史
      undoOrRedoInProgress.value = false;
      return;
    }
    // 内容变更时，清空重做栈
    redoStack.value = [];
    // 推入撤销栈
    if (oldValue !== undefined) {
      if (undoStack.value.length >= maxUndoStackSize) {
        // 超出最大容量，移除最早的记录
        undoStack.value.shift();
      }
      undoStack.value.push(oldValue);
    }
  });
});
</script>

<template>
  <main id="diary-detail">
    <section id="diary-detail-header">
      <button
          id="diary-detail-header-back-btn"
          @click="router.back()"
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
            @click="toggleMode"
            :class="{ 'active': mode === 'edit' }"
            :aria-label="mode === 'edit' ? '切换到查看模式' : '切换到编辑模式'"
        >
          <svg v-if="mode === 'edit'" viewBox="0 0 24 24" width="20" height="20">
            <path
                d="M12 4.5C7 4.5 2.73 7.61 1 12c1.73 4.39 6 7.5 11 7.5s9.27-3.11 11-7.5c-1.73-4.39-6-7.5-11-7.5zM12 17c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5zm0-8c-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3-1.34-3-3-3z"/>
          </svg>
          <svg v-else viewBox="0 0 24 24" width="20" height="20">
            <path
                d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"/>
          </svg>
        </button>

        <div class="history-controls">
          <button
              class="btn-icon undo-btn"
              @click="undo"
              :disabled="undoStack.length === 0"
              :class="{ 'disabled': undoStack.length === 0 }"
              aria-label="撤销"
          >
            <svg viewBox="0 0 24 24" width="20" height="20">
              <path
                  d="M12.5 8c-2.65 0-5.05.99-6.9 2.6L2 7v9h9l-3.62-3.62c1.39-1.16 3.16-1.88 5.12-1.88 3.54 0 6.55 2.31 7.6 5.5l2.37-.78C21.08 11.03 17.15 8 12.5 8z"/>
            </svg>
          </button>
          <button
              class="btn-icon redo-btn"
              @click="redo"
              :disabled="redoStack.length === 0"
              :class="{ 'disabled': redoStack.length === 0 }"
              aria-label="重做"
          >
            <svg viewBox="0 0 24 24" width="20" height="20">
              <path
                  d="M18.4 10.6C16.55 8.99 14.15 8 11.5 8c-4.65 0-8.58 3.03-9.96 7.22L3.9 16c1.05-3.19 4.05-5.5 7.6-5.5 1.95 0 3.73.72 5.12 1.88L13.5 16H22V7l-3.6 3.6z"/>
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
                  <path
                      d="M21 19V5c0-1.1-.9-2-2-2H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2zM8.5 13.5l2.5 3.01L14.5 12l4.5 6H5l3.5-4.5z"/>
                </svg>
                <span>图片</span>
              </button>
              <button @click="triggerAddVideo" class="media-option">
                <svg viewBox="0 0 24 24" width="16" height="16">
                  <path
                      d="M17 10.5V7c0-.55-.45-1-1-1H4c-.55 0-1 .45-1 1v10c0 .55.45 1 1 1h12c.55 0 1-.45 1-1v-3.5l4 4v-11l-4 4z"/>
                </svg>
                <span>视频</span>
              </button>
              <button @click="showAudioDrawer = true; showMediaMenu = false" class="media-option">
                <svg viewBox="0 0 24 24" width="16" height="16">
                  <path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z"/>
                  <path
                      d="M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/>
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
            @click="saveDiary()"
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
            @click="deleteDiary"
            :disabled="delLoading"
            :class="{ 'loading': delLoading }"
            aria-label="删除日记"
        >
          <span v-if="delLoading" class="loading-spinner"></span>
          <span v-else class="btn-text">删除</span>
        </button>
      </div>
    </section>

    <section id="diary-detail-main">
      <transition name="fade">
        <div v-if="renderLoading" id="loading-overlay">
          <div class="loading-content">
            <div class="loading-spinner"></div>
            <p class="loading-text">正在加载日记内容和附件...</p>
            <p class="loading-subtext" v-if="renderMsg">{{ renderMsg }}</p>
          </div>
        </div>
      </transition>

      <div class="editor-container">
        <rich-text-editor
            id="diary-editor"
            :diary="diary"
            v-model="diary.content"
            :mode="mode"
            @update:cursor-position="updateCursor"
            @process:download-attachment="msg => renderMsg = msg"
            @request:preview-media="openPreviewMedia"
        />
      </div>
    </section>

    <section id="diary-detail-footer">
      <section id="diary-detail-footer-left">
        <div class="footer-item" :title="`${contentLen} 字`">
          <svg viewBox="0 0 24 24" width="14" height="14">
            <path
                d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z"/>
          </svg>
          <span class="footer-text">{{ contentLen }}字</span>
        </div>
        <div class="footer-item" v-if="statusMsg" :title="statusMsg">
          <svg viewBox="0 0 24 24" width="14" height="14" v-if="!statusMsg.includes('⏳')">
            <path
                d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
          </svg>
          <span class="footer-text">{{ statusMsg }}</span>
        </div>
      </section>

      <section id="diary-detail-footer-right">
        <div class="footer-item" :title="formatTimestamp(diary.updated)">
          <span class="footer-emoji">{{ getCurEmoji() }}</span>
          <span class="footer-text">{{ formatTimestamp(diary.updated) }}</span>
        </div>
        <div class="footer-item" :title="formatTimestamp(diary.created)">
          <svg viewBox="0 0 24 24" width="14" height="14">
            <path
                d="M20 3h-1V1h-2v2H7V1H5v2H4c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 18H4V8h16v13z"/>
          </svg>
          <span class="footer-text">{{ formatTimestamp(diary.created) }}</span>
        </div>
      </section>
    </section>

    <capture-audio-drawer
        :visible="showAudioDrawer"
        @close="showAudioDrawer = false"
        @recorded="recordedAudio"
    />
  </main>
</template>

<style scoped lang="scss">
#diary-detail {
  width: 100%;
  height: 100%;
  max-height: 100%;
  display: flex;
  flex-direction: column;
  background-color: var(--pad-bg-color-100);
  font-family: var(--pad-font-family), serif;
  overflow: hidden;

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

  #diary-detail-main {
    flex: 1;
    position: relative;
    overflow: hidden;
    background-color: var(--pad-bg-color-100);

    .editor-container {
      width: 100%;
      height: 100%;
      overflow: hidden;

      #diary-editor {
        width: 100%;
        height: 100%;
      }
    }

    #loading-overlay {
      position: absolute;
      top: 0;
      left: 0;
      width: 100%;
      height: 100%;
      z-index: 100;
      background-color: rgba(var(--pad-bg-color-100-rgb), 0.9);
      backdrop-filter: blur(4px);
      display: flex;
      justify-content: center;
      align-items: center;

      .loading-content {
        text-align: center;
        max-width: 300px;
        padding: 32px;
        background-color: var(--pad-bg-color-200);
        border-radius: var(--pad-radius-xl);
        box-shadow: var(--pad-shadow-lg);
        border: 1px solid var(--pad-border-color-200);
      }

      .loading-spinner {
        width: 48px;
        height: 48px;
        border: 3px solid var(--pad-border-color-100);
        border-top-color: var(--pad-primary-color);
        border-radius: 50%;
        animation: spin 1s linear infinite;
        margin: 0 auto 24px;
      }

      .loading-text {
        color: var(--pad-text-color-100);
        font-weight: 500;
        font-size: 16px;
        margin-bottom: 8px;
      }

      .loading-subtext {
        color: var(--pad-text-color-300);
        font-size: 14px;
        margin: 0;
      }
    }

    .fade-enter-active,
    .fade-leave-active {
      transition: opacity var(--pad-transition-base);
    }

    .fade-enter-from,
    .fade-leave-to {
      opacity: 0;
    }
  }

  #diary-detail-footer {
    height: 48px;
    min-height: 48px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 8px;
    background-color: var(--pad-bg-color-200);
    border-top: 1px solid var(--pad-border-color-200);
    font-size: 13px;
    color: var(--pad-text-color-300);
    flex-shrink: 0;
    gap: 20px;

    #diary-detail-footer-left,
    #diary-detail-footer-right {
      display: flex;
      align-items: center;
      gap: 24px;
    }

    .footer-item {
      display: flex;
      align-items: center;
      gap: 8px;
      cursor: default;

      svg {
        fill: currentColor;
        opacity: 0.7;
      }

      .footer-emoji {
        font-size: 14px;
      }

      .footer-text {
        font-size: 12px;
        font-weight: 400;
        letter-spacing: 0.2px;
      }

      &:hover {
        color: var(--pad-text-color-200);
      }
    }

    // 状态信息样式
    #diary-detail-footer-left .footer-item:nth-child(2) {
      color: var(--pad-success-color);
      font-weight: 500;

      svg {
        fill: currentColor;
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
  #diary-detail {
    #diary-detail-header {
      height: 56px;
      gap: 12px;

      .header-controls {
        .history-controls {
          display: none; // 在移动端隐藏历史控制
        }
      }

      .header-actions {
        gap: 8px;

        button {
          padding: 8px 16px;
          font-size: 13px;
        }
      }
    }

    #diary-detail-footer {
      height: 44px;
      font-size: 12px;
      gap: 12px;

      #diary-detail-footer-left,
      #diary-detail-footer-right {
        gap: 16px;
      }

      .footer-item {
        gap: 3px;
      }
    }
  }
}

@media (max-width: 480px) {
  #diary-detail {
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

    #diary-detail-footer {
      #diary-detail-footer-left,
      #diary-detail-footer-right {
        gap: 12px;
      }
    }
  }
}

// 深色模式优化
html.dark {
  #diary-detail {
    #diary-detail-main {
      #loading-overlay {
        background-color: rgba(27, 33, 41, 0.9);
      }
    }
  }
}

// 打印样式
@media print {
  #diary-detail {
    #diary-detail-header,
    #diary-detail-footer {
      display: none;
    }

    #diary-detail-main {
      overflow: visible;
    }
  }
}
</style>
