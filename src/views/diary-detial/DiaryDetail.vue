<script setup lang="ts">
import {nextTick, onActivated, onMounted, ref, watch} from "vue";
import TiptapEditor from "../../components/TiptapEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {useDiaryCore} from "../../composables/useDiaryCore.ts";
import {onBeforeRouteLeave} from "vue-router";
import {Dialog, useQuasar} from "quasar";
import {useEditorUI} from "../../composables/useEditorUI.ts";
import {useMediaAction} from "../../composables/useMediaAction.ts";
import {formatTimestamp} from "../../utils";
import AttachmentCard from "../../components/AttachmentCard.vue";
import CaptureAudioDrawer from "../../components/CaptureAudioDrawer.vue";
import {useDataStore} from "../../stores/data.ts";
import ImagePreview from "../../components/ImagePreview.vue";
import api from "../../utils/api.ts";
import {formatError} from "../../utils/formatError.ts";
import {useConfigStore} from "../../stores/config.ts";

const $q = useQuasar();
const configStore = useConfigStore();

const tiptapEditorRef = ref<InstanceType<typeof TiptapEditor>>();
const editorDomRef = ref<HTMLElement>();
const showDetailDialog = ref(false);
const showRenameDialog = ref(false);
const oldFilename = ref('');
const newFilename = ref('');
const pinnedDiaryIds = configStore.useTauriConfig('pinned_diary_ids');
let renameCb: ((newFilename: string) => void) | null = null;

const {
  diaryId, diary, diaryContent, attachmentMap, isNew, isInitialLoaded, unusedAttachments, isDelBack,
  loadDiaryInfo, deleteDiary, updateContent
} = useDiaryCore();

// UI交互
const {
  showMenu, showToolbar, showToolbarPanel,
  setupToolbar,
} = useEditorUI();

// 媒体操作
const mediaAction = useMediaAction(diaryId, editorDomRef, showToolbarPanel, tiptapEditorRef);
const {uploadTasks, showUploadDialog, isUploading, showAudioDrawer} = mediaAction;

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

function showDiarySource() {
  $q.dialog({
    title: '日记内容 - 源码',
    message: diaryContent.value.replace('\n', '\\n'),
    persistent: true,
    ok: {label: '关闭', color: 'primary'},
  });
  showMenu.value = false;
}

function showImage(src: string) {
  Dialog.create({
    component: ImagePreview,
    componentProps: {src}
  })
}

function renameAttachment(filename: string, cb: (newFilename: string) => void) {
  showRenameDialog.value = true;
  oldFilename.value = filename;
  newFilename.value = filename;
  renameCb = cb;
}

async function handleRenameAttachment() {
  if (!newFilename.value || oldFilename.value === newFilename.value) {
    showRenameDialog.value = false;
    return;
  }
  const illegal = newFilename.value.includes('[[') || newFilename.value.includes(']]');
  if (illegal) {
    $q.notify({type: 'negative', message: `重命名失败，请去掉新文件名的特殊字符：'[[' 或 ']]'`});
    return;
  }
  try {
    // 替换 Markdown 中所有附件的 filename 引用
    const re = new RegExp(`\\[\\[(IMG|VID|AUD|FILE):${escapeRegExp(oldFilename.value)}(\\|[^\\]]*)?\\]\\]`, 'g');
    const newContent = diaryContent.value.replace(re, (_match, type, config) => {
      return config ? `[[${type}:${newFilename.value}${config}]]` : `[[${type}:${newFilename.value}]]`;
    });
    await api.cmdUpdateAttachmentFilename(
        diaryId.value,
        oldFilename.value,
        newFilename.value,
        newContent
    );
    updateAttachmentFilename(diaryId.value, oldFilename.value, newFilename.value);
    updateContent(newContent);
    showRenameDialog.value = false;
    renameCb?.(newFilename.value);
  } catch (e) {
    $q.notify({type: 'negative', message: formatError(e)});
  }
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
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

onBeforeRouteLeave((_to, _from, next) => {
  const orphans = unusedAttachments.value;
  if (!orphans.length || isDelBack.value) {
    // 删除日记后的退出不用咨询
    next();
    return;
  }
  // 先询问用户是否要删除这些未使用的附件
  $q.dialog({
    title: '未使用的附件',
    message: `有 ${orphans.length} 个未使用的附件，是否删除？`,
    persistent: true,
    ok: {label: '删除', color: 'negative'},
    cancel: {label: '保留', color: 'primary'},
  }).onOk(() => {
    Promise
        .all(orphans.map(att => api.cmdDeleteAttachment(diaryId.value, att.filename)))
        .then(() => {
          deleteAttachment(diaryId.value, orphans.map(att => att.filename));
          next();
        })
        .catch(e => {
          console.error('删除附件失败:', e);
          $q.notify({type: 'negative', message: `删除附件失败 ${e.message || e.error || e}`});
        });
  }).onCancel(() => next());
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
        :attachmentMap="attachmentMap"
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
                  @click="() => {mediaAction.cachingAttachment(diary!.attachments.map(att => att.filename)); showMenu = false}">
            <q-item-section>缓存所有附件到本地</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="showMenu = false">
            <q-item-section>取消</q-item-section>
          </q-item>
        </q-list>
      </q-card>
    </q-dialog>

    <q-dialog no-refocus v-model="showDetailDialog">
      <q-card style="min-width: 350px; max-width: 90vw;">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">{{ diary?.title }} - 详情</div>
          <q-space/>
          <q-btn icon="close" flat round dense v-close-popup/>
        </q-card-section>

        <q-card-section class="q-pa-md">
          <div class="text-subtitle2 q-mb-xs">时间信息</div>
          <div class="text-caption">创建时间：{{ formatTimestamp(diary?.created) }}</div>
          <div class="text-caption">更新时间：{{ formatTimestamp(diary?.updated) }}</div>

          <q-separator class="q-my-md"/>

          <div class="text-subtitle2 q-mb-sm">附件列表 ({{ diary?.attachments.length || 0 }})</div>
          <q-list bordered separator v-if="diary?.attachments.length">
            <AttachmentCard
                v-for="att in diary.attachments"
                :key="att.filename"
                :att="att"
            />
          </q-list>
          <div v-else class="text-center q-pa-sm">暂无附件</div>
        </q-card-section>

        <q-card-actions align="right">
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
                <q-item-label class="text-caption ellipsis">{{ task.filename }}</q-item-label>
                <q-linear-progress
                    :value="task.progress"
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
