<script setup lang="ts">
import {computed, onMounted, ref, watch} from "vue";
import DiaryHeader from "./DiaryHeader.vue";
import LiveRichEditor from "../../components/LiveRichEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {useDiaryCore} from "../../composables/useDiaryCore.ts";
import {useRoute} from "vue-router";
import {useQuasar} from "quasar";
import {useEditorUI} from "../../composables/useEditorUI.ts";
import {useMediaAction} from "../../composables/useMediaAction.ts";
import {formatBytes, formatTimestamp} from "../../utils";

const route = useRoute();
const $q = useQuasar();
const liveEditorRef = ref<InstanceType<typeof LiveRichEditor>>();
const editorDomRef = ref<HTMLElement>();
const showDetailDialog = ref(false);

const initialDiaryId = (route.params.id as string) || "";
const {
  diaryId, diary, diaryContent, isNew, isInitialLoaded,
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
  insertPhoto,
  takePhoto,
  insertAudio,
  audioRecording,
  insertVideo,
  takeVideo,
  insertFile,
} = useMediaAction(diaryId, editorDomRef, showToolbarPanel);

const canUndo = computed(() => false);
const canRedo = computed(() => false);

const BAR_MAX_HEIGHT = 56;
const editorPadding = computed(() => {
  if (showToolbar || showToolbarPanel) return `${BAR_MAX_HEIGHT + 16}px 16px`
  else return '16px'
});

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
</script>

<template>
  <main>
    <DiaryHeader
        class="header"
        :title="diary?.title"
        @back="$router.back()"
        @info="openDiaryDetail"
        @operate="showMenu = true"
        style="width: 100%; flex-shrink: 0"
        :style="{height: BAR_MAX_HEIGHT + 'px'}"
    />
    <LiveRichEditor
        ref="liveEditorRef"
        v-if="isInitialLoaded"
        v-model="diaryContent"
        :diarySummary="diary"
        style="width: 100%; flex: 1"
        :style="{padding: editorPadding}"
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
        @takeVideo="takeVideo"
        @insertFile="insertFile"
        style="width: 100%; flex-shrink: 0"
    />

    <q-dialog v-model="showMenu" position="bottom">
      <q-card class="action-sheet-card">
        <q-list padding class="text-center">
          <q-item clickable v-ripple @click="">
            <q-item-section>操作1</q-item-section>
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
            <q-item v-for="att in diary.attachments" :key="att.filename">
              <q-item-section>
                <q-item-label class="text-weight-medium">{{ att.filename }}</q-item-label>
                <q-item-label caption>
                  {{ att.mimetype }} · {{ formatBytes(att.size) }}
                </q-item-label>
              </q-item-section>
              <q-item-section side>
                <q-chip
                    :color="att.encrypted ? 'orange-2' : 'green-2'"
                    :text-color="att.encrypted ? 'orange-9' : 'green-9'"
                    size="sm"
                    dense
                >
                  {{ att.encrypted ? '已加密' : '明文' }}
                </q-chip>
              </q-item-section>
            </q-item>
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
  </main>
</template>

<style scoped lang="scss">
main {
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
