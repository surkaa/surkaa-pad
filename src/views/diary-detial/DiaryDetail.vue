<script setup lang="ts">
import {computed, nextTick, onActivated, onMounted, ref, watch} from "vue";
import TiptapEditor from "../../components/TiptapEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {useDiaryCore} from "../../composables/useDiaryCore.ts";
import {onBeforeRouteLeave, type NavigationGuardNext, useRoute} from "vue-router";
import {Dialog, useQuasar} from "quasar";
import {useEditorUI} from "../../composables/useEditorUI.ts";
import {useMediaAction} from "../../composables/useMediaAction.ts";
import CaptureAudioDrawer from "../../components/CaptureAudioDrawer.vue";
import {useDataStore} from "../../stores/data.ts";
import ImagePreview from "../../components/ImagePreview.vue";
import api from "../../utils/api.ts";
import {formatError} from "../../utils/formatError.ts";
import {useConfigStore} from "../../stores/config.ts";
import type {AttachmentMeta} from "../../bindings.ts";
import {normalizeSearchTerms} from "../../utils/searchHighlight.ts";
import DiaryInfoDialog from './DiaryInfoDialog.vue';
import DiarySourceDialog from './DiarySourceDialog.vue';
import UploadTasksDialog from './UploadTasksDialog.vue';
import UnusedAttachmentsDialog from './UnusedAttachmentsDialog.vue';
import {useDiaryAttachmentRename} from '../../composables/useDiaryAttachmentRename';
import {useDiaryEditorShortcuts} from '../../composables/useDiaryEditorShortcuts';

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

const {
  diaryId, diary, attachments, diaryContent, attachmentMap, isNew, isInitialLoaded, unusedAttachments, isDelBack,
  loadDiaryInfo, deleteDiary
} = useDiaryCore();

// UI交互
const {
  showMenu, showToolbar, showToolbarPanel,
  setupToolbar, showToolbarAfterEditorFocus,
} = useEditorUI();

// 媒体操作
const mediaAction = useMediaAction(diaryId, editorDomRef, showToolbarPanel, tiptapEditorRef);
const {uploadTasks, showUploadDialog, isUploading, showAudioDrawer} = mediaAction;
const {
  showRenameDialog,
  oldFilename,
  newFilename,
  requestRename: renameAttachment,
  closeRenameDialog,
  confirmRename: handleRenameAttachment,
} = useDiaryAttachmentRename(diaryId);
const showUnusedAttachmentsDialog = ref(false);
const unusedAttachmentActionLoading = ref(false);
const pendingUnusedAttachments = ref<AttachmentMeta[]>([]);
let pendingNavigation: NavigationGuardNext | null = null;

const {deleteAttachment} = useDataStore();

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

    <DiaryInfoDialog
        v-model="showDetailDialog"
        :diary="diary"
        :diary-id="diaryId"
        :attachments="attachments"
    />

    <DiarySourceDialog v-model="showSourceDialog" :diary-id="diaryId"/>

    <UploadTasksDialog
        v-model="showUploadDialog"
        :tasks="uploadTasks"
        :completed="isUploading"
    />

    <q-dialog no-refocus v-model="showRenameDialog" persistent>
      <q-card style="min-width: 300px; max-width: 500px">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">重命名附件</div>
        </q-card-section>

        <q-card-section class="q-pt-md">
          <q-input v-model="newFilename" label="新文件名" autofocus/>
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
