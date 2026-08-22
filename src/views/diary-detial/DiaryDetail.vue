<script setup lang="ts">
import {computed, nextTick, onActivated, onMounted, ref, watch} from "vue";
import TiptapEditor from "../../components/TiptapEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {useDiaryCore} from "../../composables/useDiaryCore.ts";
import {useRoute} from "vue-router";
import {Dialog, useQuasar} from "quasar";
import {useEditorUI} from "../../composables/useEditorUI.ts";
import {useMediaAction} from "../../composables/useMediaAction.ts";
import CaptureAudioDrawer from "../../components/CaptureAudioDrawer.vue";
import ImagePreview from "../../components/ImagePreview.vue";
import {useConfigStore} from "../../stores/config.ts";
import {normalizeSearchTerms} from "../../utils/searchHighlight.ts";
import DiaryInfoDialog from './DiaryInfoDialog.vue';
import DiarySourceDialog from './DiarySourceDialog.vue';
import UploadTasksDialog from './UploadTasksDialog.vue';
import UnusedAttachmentsDialog from './UnusedAttachmentsDialog.vue';
import {useDiaryAttachmentRename} from '../../composables/useDiaryAttachmentRename';
import {useDiaryEditorShortcuts} from '../../composables/useDiaryEditorShortcuts';
import {useDiaryLeaveGuard} from '../../composables/useDiaryLeaveGuard';
import {platform} from '@tauri-apps/plugin-os';
import {openUrl} from '@tauri-apps/plugin-opener';
import type {DiaryLocation} from '../../bindings';
import {
  buildAmapLocationUrl,
  captureCurrentDiaryLocation,
  DiaryLocationError,
} from '../../utils/diaryLocation';
import LocationConfirmDialog from '../../components/LocationConfirmDialog.vue';

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
const pinnedDiaryIds = configStore.useTauriConfig('pinned_diary_ids');
const editorShortcuts = configStore.useTauriConfig('windows_editor_shortcuts');
const toolbarOrder = configStore.useTauriConfig('editor_toolbar_order');
const isAndroid = platform() === 'android';
const showLocationDialog = ref(false);
const pendingLocation = ref<DiaryLocation | null>(null);

const {
  diaryId, diary, attachments, diaryManifestSize, diaryContent, attachmentMap, isNew, isInitialLoaded, unusedAttachments, isDelBack,
  loadDiaryInfo, deleteDiary, flushPendingSave
} = useDiaryCore();

// UI交互
const {
  showMenu, showToolbar, showToolbarPanel,
  setupToolbar, showToolbarAfterEditorFocus,
} = useEditorUI();

// 媒体操作
const mediaAction = useMediaAction(diaryId, editorDomRef, showToolbarPanel, tiptapEditorRef);
const {
  uploadTasks,
  showUploadDialog,
  allUploadsSettled,
  cancelUploadTask,
  cancelAllUploads,
  showAudioDrawer,
} = mediaAction;
const {
  showRenameDialog,
  oldFilename,
  newFilename,
  requestRename: renameAttachment,
  closeRenameDialog,
  confirmRename: handleRenameAttachment,
} = useDiaryAttachmentRename(diaryId);

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

async function captureLocation() {
  if (!isAndroid) return;
  showToolbarPanel.value = false;
  $q.loading.show({message: '正在获取当前位置…'});
  try {
    pendingLocation.value = await captureCurrentDiaryLocation();
    showLocationDialog.value = true;
  } catch (error) {
    const message = error instanceof DiaryLocationError
      ? error.message
      : `获取当前位置失败：${String(error)}`;
    $q.notify({type: 'negative', message});
  } finally {
    $q.loading.hide();
  }
}

function insertLocation(location: DiaryLocation) {
  if (!tiptapEditorRef.value?.insertLocation(location)) {
    $q.notify({type: 'negative', message: '插入当前位置失败'});
    return;
  }
  showLocationDialog.value = false;
  pendingLocation.value = null;
}

async function openLocation(location: DiaryLocation) {
  try {
    await openUrl(buildAmapLocationUrl(location));
  } catch (error) {
    $q.notify({type: 'negative', message: `打开地图失败：${String(error)}`});
  }
}

useDiaryEditorShortcuts({
  shortcuts: editorShortcuts,
  showToolbarPanel,
  isInteractionBlocked: () => Boolean(
    showMenu.value
    || showDetailDialog.value
    || showSourceDialog.value
    || showRenameDialog.value
    || showUnusedAttachmentsDialog.value
    || showUploadDialog.value
    || showAudioDrawer.value
    || showLocationDialog.value
  ),
  handlers: {
    insertPhoto: () => void mediaAction.insertPhoto(),
    insertAudio: () => void mediaAction.insertAudio(),
    audioRecording: mediaAction.audioRecording,
    insertVideo: () => void mediaAction.insertVideo(),
    insertFile: () => void mediaAction.insertFile(),
  },
});

function showDiarySource() {
  showMenu.value = false;
  showSourceDialog.value = true;
}

async function showBlockOrder() {
  showMenu.value = false;
  await nextTick();
  tiptapEditorRef.value?.openBlockOrderDialog();
}

function showImage(src: string) {
  Dialog.create({
    component: ImagePreview,
    componentProps: {src}
  })
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

const {
  showUnusedAttachmentsDialog,
  unusedAttachmentActionLoading,
  pendingUnusedAttachments,
  finishUnusedAttachmentCheck,
  appendUnusedAttachments,
  deleteUnusedAttachments,
} = useDiaryLeaveGuard({
  diaryId,
  unusedAttachments,
  isDeletingDiary: isDelBack,
  hasActiveUploads: mediaAction.hasActiveUploads,
  showUploadDialog,
  cancelAllUploads,
  insertExistingAttachmentsAtEnd: mediaAction.insertExistingAttachmentsAtEnd,
  flushPendingSave,
});

onMounted(async () => {
  const shouldFocusEditor = isNew.value;
  if (!shouldFocusEditor) {
    await loadDiaryInfo();
  } else {
    // 新建日记，直接标记加载完成，允许保存
    isInitialLoaded.value = true;
  }
  setupToolbar();
  if (shouldFocusEditor) {
    await nextTick();
    tiptapEditorRef.value?.focusEnd();
  }
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
        @openLocation="openLocation"
        style="width: 100%; flex: 1; padding: 16px"
    />

    <EditToolbar
        :view="showToolbar || showToolbarPanel"
        :panelOpen="showToolbarPanel"
        :editor="tiptapEditorRef?.editor ?? null"
        :shortcuts="editorShortcuts"
        :toolbar-order="toolbarOrder"
        v-click-outside="() => showToolbarPanel = false"
        @undo="tiptapEditorRef?.undo"
        @redo="tiptapEditorRef?.redo"
        @editSummary="tiptapEditorRef?.openSummaryDialog"
        @additionalAction="additionalAction"
        @insertPhoto="mediaAction.insertPhoto"
        @takePhoto="mediaAction.takePhoto"
        @insertAudio="mediaAction.insertAudio"
        @audioRecording="mediaAction.audioRecording"
        @insertVideo="mediaAction.insertVideo"
        @insertFile="mediaAction.insertFile"
        @insertLocation="captureLocation"
        style="width: 100%; flex-shrink: 0"
    />

    <CaptureAudioDrawer
        :visible="showAudioDrawer"
        @close="showAudioDrawer = false"
        @recorded="mediaAction.handleAudioRecorded"
    />

    <LocationConfirmDialog
        v-model="showLocationDialog"
        :location="pendingLocation"
        @retry="captureLocation"
        @confirm="insertLocation"
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
          <q-item clickable v-ripple @click="showBlockOrder">
            <q-item-section>调整内容顺序</q-item-section>
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

    <DiaryInfoDialog
        v-model="showDetailDialog"
        :diary="diary"
        :diary-id="diaryId"
        :attachments="attachments"
        :manifest-size="diaryManifestSize"
    />

    <DiarySourceDialog v-model="showSourceDialog" :diary-id="diaryId"/>

    <UploadTasksDialog
        v-model="showUploadDialog"
        :tasks="uploadTasks"
        :all-settled="allUploadsSettled"
        @cancel="cancelUploadTask"
        @cancel-all="cancelAllUploads"
    />

    <q-dialog no-refocus v-model="showRenameDialog" persistent>
      <q-card style="min-width: 300px; max-width: 500px">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">重命名附件</div>
        </q-card-section>

        <q-card-section class="q-pt-md">
          <q-input
            v-model="newFilename"
            label="新文件名"
            input-class="rename-attachment-filename-input"
            autofocus
          />
        </q-card-section>

        <q-card-actions align="right">
          <q-btn flat label="取消" color="primary" @click="closeRenameDialog"/>
          <q-btn unelevated label="重命名" color="primary" :disable="!newFilename || newFilename === oldFilename"
                 @click="handleRenameAttachment"/>
        </q-card-actions>
      </q-card>
    </q-dialog>

    <UnusedAttachmentsDialog
        v-model="showUnusedAttachmentsDialog"
        :count="pendingUnusedAttachments.length"
        :loading="unusedAttachmentActionLoading"
        @keep="finishUnusedAttachmentCheck"
        @append="appendUnusedAttachments"
        @delete="deleteUnusedAttachments"
    />
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

}
</style>

<style lang="scss">
.rename-attachment-filename-input {
  color: var(--pad-text-color-100) !important;
  caret-color: var(--pad-primary-dark);
  -webkit-text-fill-color: var(--pad-text-color-100);
}
</style>
