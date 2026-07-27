<script setup lang="ts">
import {computed, nextTick, onActivated, onMounted, ref, watch} from "vue";
import TiptapEditor from "../../components/TiptapEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {useDiaryCore} from "../../composables/useDiaryCore.ts";
import {onBeforeRouteLeave, type NavigationGuardNext, useRoute} from "vue-router";
import {Dialog, useQuasar} from "quasar";
import {useEditorUI} from "../../composables/useEditorUI.ts";
import {type UploadTask, useMediaAction} from "../../composables/useMediaAction.ts";
import {formatTimestamp} from "../../utils";
import AttachmentCard from "../../components/AttachmentCard.vue";
import CaptureAudioDrawer from "../../components/CaptureAudioDrawer.vue";
import {useDataStore} from "../../stores/data.ts";
import ImagePreview from "../../components/ImagePreview.vue";
import api from "../../utils/api.ts";
import {formatError} from "../../utils/formatError.ts";
import {useConfigStore} from "../../stores/config.ts";
import type {AttachmentMeta} from "../../bindings.ts";
import {normalizeSearchTerms} from "../../utils/searchHighlight.ts";
import {platform} from "@tauri-apps/plugin-os";
import {useEventListener} from "@vueuse/core";
import {attachmentGroupIcon, groupAttachmentsByMimeType} from "../../utils/attachmentGrouping.ts";
import {copyTextToClipboard} from "../../utils/clipboard.ts";
import {
  findEditorShortcutAction,
  type EditorShortcutAction,
} from "../../utils/editorShortcuts.ts";

const $q = useQuasar();
const configStore = useConfigStore();
const route = useRoute();
const searchTerms = computed(() => normalizeSearchTerms(
    typeof route.query.highlight === 'string' ? route.query.highlight : '',
));

const tiptapEditorRef = ref<InstanceType<typeof TiptapEditor>>();
const editorDomRef = ref<HTMLElement>();
const showDetailDialog = ref(false);
const showSourceDialog = ref(false);
const diaryManifestSource = ref('');
const diarySourceLoading = ref(false);
const showRenameDialog = ref(false);
const renameAttachmentId = ref('');
const oldFilename = ref('');
const newFilename = ref('');
const pinnedDiaryIds = configStore.useTauriConfig('pinned_diary_ids');
const editorShortcuts = configStore.useTauriConfig('windows_editor_shortcuts');
const isWindows = platform() === 'windows';
let renameCb: ((newFilename: string) => void) | null = null;

const {
  diaryId, diary, attachments, diaryContent, attachmentMap, isNew, isInitialLoaded, unusedAttachments, isDelBack,
  loadDiaryInfo, deleteDiary
} = useDiaryCore();
const attachmentGroups = computed(() => groupAttachmentsByMimeType(attachments.value));
const expandedAttachmentGroups = ref<Record<string, boolean>>({});

// UI交互
const {
  showMenu, showToolbar, showToolbarPanel,
  setupToolbar, showToolbarAfterEditorFocus,
} = useEditorUI();

// 媒体操作
const mediaAction = useMediaAction(diaryId, editorDomRef, showToolbarPanel, tiptapEditorRef);
const {uploadTasks, showUploadDialog, isUploading, showAudioDrawer} = mediaAction;
const showUnusedAttachmentsDialog = ref(false);
const unusedAttachmentActionLoading = ref(false);
const pendingUnusedAttachments = ref<AttachmentMeta[]>([]);
let pendingNavigation: NavigationGuardNext | null = null;

const {deleteAttachment, updateAttachmentFilename} = useDataStore();

function openDiaryDetail() {
  if (!diary.value) {
    $q.notify({type: 'negative', message: '无法获取日记详情'});
    return;
  }
  showDetailDialog.value = true;
}

function additionalAction() {
  showToolbarPanel.value = !showToolbarPanel.value;
}

function uploadTaskStatusText(task: UploadTask) {
  if (task.status === 'completed') return '已完成';
  if (task.status === 'error') return '上传失败';
  if (task.phase === 'finalizing') return '正在完成：提交附件并保存日记';
  if (task.status === 'pending') return '准备中';
  return `上传中 ${Math.round(task.progress * 100)}%`;
}

function isEditableFieldOutsideDiaryEditor(target: EventTarget | null) {
  const element = target instanceof Element ? target : null;
  if (!element || element.closest('.ProseMirror')) return false;
  return Boolean(element.closest('input, textarea, select, [contenteditable="true"]'));
}

function runEditorShortcut(action: EditorShortcutAction) {
  showToolbarPanel.value = false;
  switch (action) {
    case 'insertPhoto':
      void mediaAction.insertPhoto();
      break;
    case 'insertAudio':
      void mediaAction.insertAudio();
      break;
    case 'audioRecording':
      mediaAction.audioRecording();
      break;
    case 'insertVideo':
      void mediaAction.insertVideo();
      break;
    case 'insertFile':
      void mediaAction.insertFile();
      break;
  }
}

function handleEditorShortcut(event: KeyboardEvent) {
  if (
    event.repeat
    || event.isComposing
    || route.name !== 'DiaryDetail'
    || isEditableFieldOutsideDiaryEditor(event.target)
    || showMenu.value
    || showDetailDialog.value
    || showSourceDialog.value
    || showRenameDialog.value
    || showUnusedAttachmentsDialog.value
    || showUploadDialog.value
    || showAudioDrawer.value
  ) return;

  const action = findEditorShortcutAction(event, editorShortcuts.value);
  if (!action) return;

  event.preventDefault();
  event.stopPropagation();
  runEditorShortcut(action);
}

if (isWindows) {
  useEventListener(window, 'keydown', handleEditorShortcut, {capture: true});
}

async function showDiarySource() {
  showMenu.value = false;
  showSourceDialog.value = true;
  diarySourceLoading.value = true;
  diaryManifestSource.value = '';
  try {
    const manifest = await api.cmdGetDiaryManifest(diaryId.value);
    diaryManifestSource.value = JSON.stringify(manifest, null, 2);
  } catch (error) {
    showSourceDialog.value = false;
    $q.notify({type: 'negative', message: `加载完整 Manifest 失败：${formatError(error)}`});
  } finally {
    diarySourceLoading.value = false;
  }
}

async function copyDiaryManifest() {
  if (!diaryManifestSource.value) return;
  try {
    await copyTextToClipboard(diaryManifestSource.value);
    $q.notify({type: 'positive', message: '完整 Manifest 已复制'});
  } catch (error) {
    $q.notify({type: 'negative', message: `复制 Manifest 失败：${formatError(error)}`});
  }
}

function showImage(src: string) {
  Dialog.create({
    component: ImagePreview,
    componentProps: {src}
  })
}

function renameAttachment(attachmentId: string, filename: string, cb: (newFilename: string) => void) {
  showRenameDialog.value = true;
  renameAttachmentId.value = attachmentId;
  oldFilename.value = filename;
  newFilename.value = filename;
  renameCb = cb;
}

async function handleRenameAttachment() {
  if (!newFilename.value || oldFilename.value === newFilename.value) {
    showRenameDialog.value = false;
    return;
  }
  try {
    await api.cmdUpdateAttachmentFilename(
        diaryId.value,
        renameAttachmentId.value,
        newFilename.value
    );
    updateAttachmentFilename(diaryId.value, renameAttachmentId.value, newFilename.value);
    showRenameDialog.value = false;
    renameCb?.(newFilename.value);
  } catch (e) {
    $q.notify({type: 'negative', message: formatError(e)});
  }
}

async function pinnedDiary() {
  if (pinnedDiaryIds.value.includes(diaryId.value)) {
    $q.notify({type: 'info', message: '该日记已经置顶了'});
    return;
  }
  pinnedDiaryIds.value = [...pinnedDiaryIds.value, diaryId.value];
  $q.notify({type: 'positive', message: '已置顶该日记'});
  showMenu.value = false;
}

async function unpinnedDiary() {
  const idx = pinnedDiaryIds.value.indexOf(diaryId.value);
  if (idx == -1) {
    $q.notify({type: 'info', message: '该日记未曾置顶'});
    return;
  }
  pinnedDiaryIds.value = pinnedDiaryIds.value.filter(id => id !== diaryId.value);
  $q.notify({type: 'positive', message: '已取消置顶该日记'});
  showMenu.value = false;
}

defineOptions({name: 'DiaryDetail'});

function finishUnusedAttachmentCheck() {
  showUnusedAttachmentsDialog.value = false;
  pendingUnusedAttachments.value = [];
  const next = pendingNavigation;
  pendingNavigation = null;
  next?.();
}

async function appendUnusedAttachments() {
  unusedAttachmentActionLoading.value = true;
  try {
    const inserted = await mediaAction.insertExistingAttachmentsAtEnd(pendingUnusedAttachments.value);
    if (!inserted) {
      throw new Error('编辑器未能插入附件');
    }
    await nextTick();
    finishUnusedAttachmentCheck();
  } catch (error) {
    $q.notify({type: 'negative', message: `添加附件失败：${formatError(error)}`});
  } finally {
    unusedAttachmentActionLoading.value = false;
  }
}

async function deleteUnusedAttachments() {
  unusedAttachmentActionLoading.value = true;
  const attachments = [...pendingUnusedAttachments.value];
  try {
    await Promise.all(attachments.map(att => api.cmdDeleteAttachment(diaryId.value, att.id)));
    deleteAttachment(diaryId.value, attachments.map(att => att.id));
    finishUnusedAttachmentCheck();
  } catch (error) {
    console.error('删除附件失败:', error);
    $q.notify({type: 'negative', message: `删除附件失败：${formatError(error)}`});
  } finally {
    unusedAttachmentActionLoading.value = false;
  }
}

onBeforeRouteLeave((_to, _from, next) => {
  const orphans = unusedAttachments.value;
  if (!orphans.length || isDelBack.value) {
    // 删除日记后的退出不用咨询
    next();
    return;
  }
  pendingUnusedAttachments.value = [...orphans];
  pendingNavigation = next;
  showUnusedAttachmentsDialog.value = true;
});

onMounted(async () => {
  if (!isNew.value) {
    await loadDiaryInfo();
  } else {
    // 新建日记，直接标记加载完成，允许保存
    isInitialLoaded.value = true;
  }
  setupToolbar();
});

watch(() => tiptapEditorRef.value?.editor, (newEditor) => {
  if (newEditor) {
    editorDomRef.value = newEditor.view.dom as HTMLElement;
  }
});

onActivated(async () => {
  await nextTick();
});
</script>

<template>
  <div id="diary-detail">
    <Teleport defer to="#header-actions">
      <q-btn flat round dense icon="info" @click="openDiaryDetail" aria-label="详细信息"/>
      <q-btn flat round dense icon="more_horiz" @click="showMenu = true" aria-label="操作"/>
    </Teleport>

    <TiptapEditor
        ref="tiptapEditorRef"
        v-if="isInitialLoaded"
        v-model="diaryContent"
        :diarySummary="diary"
        :attachments="attachments"
        :attachmentMap="attachmentMap"
        :searchTerms="searchTerms"
        @editorFocused="showToolbarAfterEditorFocus"
        @pasteAttachments="mediaAction.pasteAttachments"
        @showImage="showImage"
        @toggleAttachmentEncryption="mediaAction.toggleAttachmentEncryption"
        @rotateAttachment="mediaAction.rotateAttachment"
        @renameAttachment="renameAttachment"
        @saveDecryptAttachment="mediaAction.saveDecryptAttachment"
        style="width: 100%; flex: 1; padding: 16px"
    />

    <EditToolbar
        :view="showToolbar || showToolbarPanel"
        :panelOpen="showToolbarPanel"
        :editor="tiptapEditorRef?.editor ?? null"
        :shortcuts="editorShortcuts"
        v-click-outside="() => showToolbarPanel = false"
        @undo="tiptapEditorRef?.undo"
        @redo="tiptapEditorRef?.redo"
        @additionalAction="additionalAction"
        @insertPhoto="mediaAction.insertPhoto"
        @takePhoto="mediaAction.takePhoto"
        @insertAudio="mediaAction.insertAudio"
        @audioRecording="mediaAction.audioRecording"
        @insertVideo="mediaAction.insertVideo"
        @insertFile="mediaAction.insertFile"
        style="width: 100%; flex-shrink: 0"
    />

    <CaptureAudioDrawer
        :visible="showAudioDrawer"
        @close="showAudioDrawer = false"
        @recorded="mediaAction.handleAudioRecorded"
    />

    <q-dialog no-refocus v-model="showMenu" position="bottom">
      <q-card class="action-sheet-card">
        <q-list padding class="text-center">
          <q-item v-if="!pinnedDiaryIds.includes(diaryId)" clickable v-ripple @click="pinnedDiary">
            <q-item-section>置顶该日记</q-item-section>
          </q-item>
          <q-item v-else clickable v-ripple @click="unpinnedDiary">
            <q-item-section>取消置顶该日记</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="showDiarySource">
            <q-item-section>展示源码</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="() => {deleteDiary(); showMenu = false}">
            <q-item-section>删除</q-item-section>
          </q-item>
          <q-item :disable="diary == undefined" clickable v-ripple
                  @click="() => {mediaAction.cachingAttachment(attachments.map(att => att.id)); showMenu = false}">
            <q-item-section>缓存所有附件到本地</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="showMenu = false">
            <q-item-section>取消</q-item-section>
          </q-item>
        </q-list>
      </q-card>
    </q-dialog>

    <q-dialog no-refocus v-model="showDetailDialog">
      <q-card class="diary-detail-dialog">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">{{ diary?.title }} - 详情</div>
          <q-space/>
          <q-btn icon="close" flat round dense v-close-popup/>
        </q-card-section>

        <q-card-section class="q-pa-md diary-detail-dialog-content">
          <div class="text-subtitle2 q-mb-xs">基本信息</div>
          <div class="text-caption diary-id-row">
            <span>ID：</span>
            <code class="diary-id-value">{{ diaryId }}</code>
          </div>
          <div class="text-caption">创建时间：{{ formatTimestamp(diary?.created) }}</div>
          <div class="text-caption">更新时间：{{ formatTimestamp(diary?.updated) }}</div>

          <q-separator class="q-my-md"/>

          <div class="text-subtitle2 q-mb-sm">附件列表 ({{ attachments.length }})</div>
          <q-list bordered v-if="attachments.length" class="attachment-groups-list">
            <q-expansion-item
                v-for="group in attachmentGroups"
                :key="group.type"
                v-model="expandedAttachmentGroups[group.type]"
                :icon="attachmentGroupIcon(group.type)"
                :label="`${group.type} (${group.attachments.length})`"
                header-class="attachment-group-header"
            >
              <q-list separator v-if="expandedAttachmentGroups[group.type]">
                <AttachmentCard
                    v-for="att in group.attachments"
                    :key="att.id"
                    :att="att"
                />
              </q-list>
            </q-expansion-item>
          </q-list>
          <div v-else class="text-center q-pa-sm">暂无附件</div>
        </q-card-section>

        <q-card-actions align="right">
          <q-btn flat label="关闭" color="primary" v-close-popup/>
        </q-card-actions>
      </q-card>
    </q-dialog>

    <q-dialog no-refocus v-model="showSourceDialog" persistent>
      <q-card class="diary-source-dialog-card">
        <q-card-section class="row items-center q-pb-sm">
          <div class="text-h6">完整 Manifest</div>
          <q-space/>
          <q-btn icon="close" flat round dense v-close-popup aria-label="关闭源码弹窗"/>
        </q-card-section>

        <q-separator/>

        <q-card-section class="diary-source-content">
          <div v-if="diarySourceLoading" class="column items-center justify-center full-height q-gutter-sm">
            <q-spinner color="primary" size="32px"/>
            <div class="text-caption diary-source-loading-text">正在读取完整 Manifest...</div>
          </div>
          <pre v-else class="diary-manifest-source">{{ diaryManifestSource }}</pre>
        </q-card-section>

        <q-separator/>

        <q-card-actions align="right">
          <q-btn
              flat
              icon="content_copy"
              label="复制完整 Manifest"
              color="primary"
              :disable="diarySourceLoading || !diaryManifestSource"
              @click="copyDiaryManifest"
          />
          <q-btn flat label="关闭" color="primary" v-close-popup/>
        </q-card-actions>
      </q-card>
    </q-dialog>

    <!-- 上传操作不允许关闭，在完成之前 -->
    <q-dialog no-refocus v-model="showUploadDialog" persistent>
      <q-card style="min-width: 300px; max-width: 500px">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">文件处理中</div>
        </q-card-section>

        <q-card-section class="q-pt-md">
          <q-list dense>
            <q-item v-for="task in uploadTasks" :key="task.filename" class="q-px-none">
              <q-item-section>
                <q-item-label class="text-caption ellipsis upload-task-filename">
                  {{ task.filename }}
                </q-item-label>
                <q-item-label caption class="upload-task-status">
                  {{ uploadTaskStatusText(task) }}
                </q-item-label>
                <q-linear-progress
                    :value="task.progress"
                    :indeterminate="task.phase === 'finalizing' && task.status === 'uploading'"
                    :color="task.status === 'error' ? 'negative' : 'primary'"
                    class="q-mt-sm"
                    :animation-speed="200"
                />
              </q-item-section>
              <q-item-section side>
                <q-icon
                    :name="task.status === 'completed' ? 'check_circle' : (task.status === 'error' ? 'error' : 'cloud_upload')"
                    :color="task.status === 'completed' ? 'positive' : (task.status === 'error' ? 'negative' : 'grey')"
                />
              </q-item-section>
            </q-item>
          </q-list>
        </q-card-section>

        <q-card-actions align="right">
          <q-btn
              flat
              label="完成"
              color="primary"
              v-close-popup
              :disable="!isUploading"
          />
        </q-card-actions>
      </q-card>
    </q-dialog>

    <q-dialog no-refocus v-model="showRenameDialog" persistent>
      <q-card style="min-width: 300px; max-width: 500px">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">重命名附件</div>
        </q-card-section>

        <q-card-section class="q-pt-md">
          <q-input v-model="newFilename" label="新文件名" autofocus/>
        </q-card-section>

        <q-card-actions align="right">
          <q-btn flat label="取消" color="primary" v-close-popup/>
          <q-btn unelevated label="重命名" color="primary" :disable="!newFilename || newFilename === oldFilename"
                 @click="handleRenameAttachment"/>
        </q-card-actions>
      </q-card>
    </q-dialog>

    <q-dialog no-refocus v-model="showUnusedAttachmentsDialog" persistent>
      <q-card class="unused-attachments-dialog">
        <q-card-section>
          <div class="text-h6">未使用的附件</div>
          <div class="q-mt-sm text-body2">
            有 {{ pendingUnusedAttachments.length }} 个附件没有出现在正文中，请选择处理方式。
          </div>
        </q-card-section>

        <q-card-actions align="right" class="unused-attachment-actions">
          <q-btn
              flat
              label="保留"
              color="primary"
              :disable="unusedAttachmentActionLoading"
              @click="finishUnusedAttachmentCheck"
          />
          <q-btn
              unelevated
              label="添加到日记末尾"
              color="primary"
              :loading="unusedAttachmentActionLoading"
              @click="appendUnusedAttachments"
          />
          <q-btn
              flat
              label="删除附件"
              color="negative"
              :disable="unusedAttachmentActionLoading"
              @click="deleteUnusedAttachments"
          />
        </q-card-actions>
      </q-card>
    </q-dialog>
  </div>
</template>

<style scoped lang="scss">
#diary-detail {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;

  .action-sheet-card {
    width: 100%;
    overflow: hidden;
  }

  .unused-attachments-dialog {
    width: min(440px, 90vw);
  }

  .unused-attachment-actions {
    gap: 4px;
  }

}

// q-dialog 会被 Teleport 到 #diary-detail 外，相关样式不能嵌套在上面的选择器中。
.upload-task-filename {
  color: var(--pad-text-color-200) !important;
}

.upload-task-status {
  color: var(--pad-text-color-300) !important;
}
</style>

<style lang="scss">
.diary-detail-dialog {
  width: min(640px, 92vw);
  max-height: 90vh;
}

.diary-detail-dialog-content {
  max-height: calc(90vh - 112px);
  overflow-y: auto;
}

.diary-id-row {
  display: flex;
  align-items: baseline;
}

.diary-id-value {
  color: var(--pad-text-color-200);
  overflow-wrap: anywhere;
  user-select: all;
}

.attachment-groups-list {
  border-color: var(--pad-border-color) !important;
}

.attachment-group-header {
  color: var(--pad-text-color-200);
}

.diary-source-dialog-card {
  display: flex;
  flex-direction: column;
  width: min(900px, 94vw);
  height: min(720px, 86vh);
}

.diary-source-content {
  flex: 1;
  min-height: 0;
  padding: 0;
}

.diary-source-loading-text {
  color: var(--pad-text-color-300);
}

.diary-manifest-source {
  width: 100%;
  height: 100%;
  margin: 0;
  padding: 16px;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-200);
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
}
</style>
