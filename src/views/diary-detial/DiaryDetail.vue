<script setup lang="ts">
import {computed, nextTick, onActivated, onMounted, ref, watch} from "vue";
import LiveRichEditor from "../../components/LiveRichEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {useDiaryCore} from "../../composables/useDiaryCore.ts";
import {onBeforeRouteLeave, useRoute} from "vue-router";
import {useQuasar} from "quasar";
import {useEditorUI} from "../../composables/useEditorUI.ts";
import {useMediaAction} from "../../composables/useMediaAction.ts";
import {formatTimestamp} from "../../utils";
import AttachmentCard from "../../components/AttachmentCard.vue";
import {commands} from "../../bindings.ts";
import CaptureAudioDrawer from "../../components/CaptureAudioDrawer.vue";
import {useEventBus} from "@vueuse/core";
import {DiaryChangedEvent} from "../../types";

const route = useRoute();
const $q = useQuasar();
const liveEditorRef = ref<InstanceType<typeof LiveRichEditor>>();
const editorDomRef = ref<HTMLElement>();
const showDetailDialog = ref(false);

const initialDiaryId = (route.params.id as string) || "";
const {
  diaryId, diary, diaryContent, isNew, isInitialLoaded, unusedAttachments, isDelBack,
  loadDiaryInfo, deleteDiary
} = useDiaryCore(initialDiaryId);

// UI交互
const {
  showMenu, showToolbar, showToolbarPanel,
  setupToolbar,
} = useEditorUI();

// 媒体操作
const {
  uploadTasks,
  showUploadDialog,
  isUploading,
  showAudioDrawer,
  handleAudioRecorded,
  insertPhoto,
  takePhoto,
  insertAudio,
  audioRecording,
  insertVideo,
  insertFile,
} = useMediaAction(diaryId, editorDomRef, showToolbarPanel);

const bus = useEventBus<DiaryChangedEvent>('diary-changed');
const canUndo = computed(() => false);
const canRedo = computed(() => false);

function openDiaryDetail() {
  if (!diary.value) {
    $q.notify({type: 'negative', message: '无法获取日记详情'});
    return;
  }
  showDetailDialog.value = true;
}

function additionalAction() {
  showToolbarPanel.value = !showToolbarPanel.value;
  if (liveEditorRef.value?.editor) {
    liveEditorRef.value?.editor.focus();
  }
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
    console.log('删除附件:', orphans);
    Promise
        .all(orphans.map(att => commands.cmdDeleteAttachment(diaryId.value, att.filename)))
        .then(() => {
          bus.emit({
            type: "updated",
            summary: {
              id: diaryId.value,
              title: diary.value?.title || '',
              created: diary.value?.created || 0,
              updated: diary.value?.updated || 0,
              attachments: diary.value?.attachments.filter(att => !orphans.some(o => o.filename === att.filename)) || [],
            }
          })
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

watch(() => liveEditorRef.value?.editor, (newEditor) => {
  if (newEditor) {
    editorDomRef.value = newEditor;
  }
});

onActivated(async () => {
  await nextTick();
});
</script>

<template>
  <div id="diary-detail">
    <Teleport defer to="#header-actions">
      <button class="icon-btn" @click="openDiaryDetail" aria-label="详细信息">
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10"/>
          <path d="M12 16v-4"/>
          <path d="M12 8h.01"/>
        </svg>
      </button>
      <button class="icon-btn" @click="showMenu = true" aria-label="操作">
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="1"/>
          <circle cx="19" cy="12" r="1"/>
          <circle cx="5" cy="12" r="1"/>
        </svg>
      </button>
    </Teleport>

    <LiveRichEditor
        ref="liveEditorRef"
        v-if="isInitialLoaded"
        v-model="diaryContent"
        :diarySummary="diary"
        style="width: 100%; flex: 1; padding: 16px"
    />

    <EditToolbar
        :view="showToolbar || showToolbarPanel"
        :panelOpen="showToolbarPanel"
        :undo="canUndo"
        :redo="canRedo"
        v-click-outside="() => showToolbarPanel = false"
        @additionalAction="additionalAction"
        @insertPhoto="insertPhoto"
        @takePhoto="takePhoto"
        @insertAudio="insertAudio"
        @audioRecording="audioRecording"
        @insertVideo="insertVideo"
        @insertFile="insertFile"
        style="width: 100%; flex-shrink: 0"
    />

    <CaptureAudioDrawer
        :visible="showAudioDrawer"
        @close="showAudioDrawer = false"
        @recorded="handleAudioRecorded"
    />

    <q-dialog v-model="showMenu" position="bottom">
      <q-card class="action-sheet-card">
        <q-list padding class="text-center">
          <q-item clickable v-ripple @click="showDiarySource">
            <q-item-section>展示源码</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="() => {deleteDiary(); showMenu = false}">
            <q-item-section>删除</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="showMenu = false">
            <q-item-section>取消</q-item-section>
          </q-item>
        </q-list>
      </q-card>
    </q-dialog>

    <q-dialog v-model="showDetailDialog">
      <q-card style="min-width: 350px; max-width: 90vw;">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">{{ diary?.title }} - 详情</div>
          <q-space/>
          <q-btn icon="close" flat round dense v-close-popup/>
        </q-card-section>

        <q-card-section class="q-pa-md">
          <div class="text-subtitle2 q-mb-xs">时间信息</div>
          <div class="text-caption text-grey-8">创建时间：{{ formatTimestamp(diary?.created) }}</div>
          <div class="text-caption text-grey-8">更新时间：{{ formatTimestamp(diary?.updated) }}</div>

          <q-separator class="q-my-md"/>

          <div class="text-subtitle2 q-mb-sm">附件列表 ({{ diary?.attachments.length || 0 }})</div>
          <q-list bordered separator v-if="diary?.attachments.length">
            <AttachmentCard
                v-for="att in diary.attachments"
                :key="att.filename"
                :att="att"
            />
          </q-list>
          <div v-else class="text-center text-grey q-pa-sm">暂无附件</div>
        </q-card-section>

        <q-card-actions align="right">
          <q-btn flat label="关闭" color="primary" v-close-popup/>
        </q-card-actions>
      </q-card>
    </q-dialog>

    <q-dialog v-model="showUploadDialog" persistent>
      <q-card style="min-width: 300px; max-width: 500px">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">文件上传中</div>
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
