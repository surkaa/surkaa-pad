<script setup lang="ts">
import {computed, onMounted, ref, watch} from "vue";
import {DiaryManifest, DownloadAttachmentEvent} from "../../types";
import {invoke} from "@tauri-apps/api/core";
import {onBeforeRouteLeave, useRouter} from "vue-router";
import {showToast} from "../../utils";
import RichTextEditor from "../../components/RichTextEditor.vue";
import {saveAttachment} from "../../utils";
import CaptureAudioDrawer from "../../components/CaptureAudioDrawer.vue";
import DiaryHeader from "./DiaryHeader.vue";
import DiaryFooter from "./DiaryFooter.vue";

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
const downType = ref<DownloadAttachmentEvent['event'] | null>(null);
const renderMsg = ref('');
const isDelBack = ref(false);
const showAudioDrawer = ref(false);

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
const cursorPosition = ref<number>();
const lastSavedContent = ref("");

const contentLen = computed(() => {
  // 去除掉<<tag:filename>>标记、换行、空格后的纯文本长度
  if (!diary.value.content) return 0;
  const textOnly = diary.value.content.replace(/<<[A-Z]{3}:[^>]+>>/g, '').replace(/\s+/g, '');
  return textOnly.length;
});

function toggleMode() {
  mode.value = (mode.value === 'edit' ? 'view' : 'edit');
}

function updateDownMsg(type: DownloadAttachmentEvent['event'], msg: string) {
  renderMsg.value = msg;
  if (type =='completed') {
    downType.value = null;
    return;
  }
  downType.value = type;
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

// 处理来自Header的文件选择上传
function handleFileUpload({ tagPrefix, file }: { tagPrefix: string, file: File }) {
  console.log("选择的文件: ", file);
  uploadAttachment(tagPrefix, file.type, file.stream()).then(() => {
    // 处理完成，无需额外操作，input值清理已在子组件完成
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
    const marker = `<<${tagPrefix}:${newFile.filename}>>`;
    const content = diary.value.content || "";
    const p = cursorPosition.value ? cursorPosition.value : content.length;
    const before = content.slice(0, p);
    const after = content.slice(p);
    const prefix = before.length === 0 || before.endsWith('\n') ? '' : '\n';
    const suffix = after.length === 0 || after.startsWith('\n') ? '' : '\n';
    diary.value.content = before + prefix + marker + suffix + after;
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
    <DiaryHeader
        :mode="mode"
        :is-new="isNew"
        :save-loading="saveLoading"
        :del-loading="delLoading"
        :undo-stack-length="undoStack.length"
        :redo-stack-length="redoStack.length"
        @toggle-mode="toggleMode"
        @undo="undo"
        @redo="redo"
        @save="saveDiary"
        @delete="deleteDiary"
        @open-audio-drawer="showAudioDrawer = true"
        @upload-file="handleFileUpload"
    />

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
            @process:download-attachment="updateDownMsg"
            @request:preview-media="openPreviewMedia"
        />
      </div>
    </section>

    <DiaryFooter
        :content-len="contentLen"
        :down-type="downType"
        :status-msg="statusMsg"
        :updated="diary.updated"
        :created="diary.created"
    />

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
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
