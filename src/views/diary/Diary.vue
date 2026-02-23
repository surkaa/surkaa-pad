<script setup lang="ts">
import {computed, onMounted, ref, watch} from "vue";
import DiaryHeader from "./DiaryHeader.vue";
import LiveRichEditor from "../../components/LiveRichEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {formatTimestamp} from "../../utils";
import {useDiaryCore} from "../../composables/useDiaryCore.ts";
import {useRoute} from "vue-router";
import {useQuasar} from "quasar";
import {useEditorUI} from "../../composables/useEditorUI.ts";
import {useMediaAction} from "../../composables/useMediaAction.ts";

const route = useRoute();
const $q = useQuasar();
const liveEditorRef = ref<InstanceType<typeof LiveRichEditor>>();
const editorDomRef = ref<HTMLElement>();

const initialDiaryId = (route.params.id as string) || "";
const {
  diary, diaryContent, isNew, isInitialLoaded,
  loadDiaryInfo, deleteDiary
} = useDiaryCore(initialDiaryId);

// UI交互
const {
  showMenu, showToolbar, showToolbarPanel,
  setupToolbar,
} = useEditorUI();

// 媒体操作
const mediaActions = useMediaAction(initialDiaryId, editorDomRef);

const canUndo = computed(() => false);
const canRedo = computed(() => false);

const BAR_MAX_HEIGHT = 56;
const editorPadding = computed(() => {
  if (showToolbar || showToolbarPanel) return `${BAR_MAX_HEIGHT + 16}px 16px`
  else return '16px'
});

function showDiaryDetail() {
  if (!diary.value) {
    $q.notify({message: '日记信息未加载'});
    return;
  }
  const {title, created, updated, attachments} = diary.value;
  $q.dialog({
    title,
    message: `创建时间：${formatTimestamp(created)}<br>更新时间：${formatTimestamp(updated)}<br>附件数量：${attachments.length}`,
    html: true,
    ok: {label: '关闭', color: 'primary', flat: true},
  });
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
        @info="showDiaryDetail"
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
        @additionalAction="showToolbarPanel = !showToolbarPanel"
        @insertPhoto="mediaActions.insertPhoto"
        @takePhoto="mediaActions.takePhoto"
        @audioRecording="mediaActions.audioRecording"
        @insertVideo="mediaActions.insertVideo"
        @takeVideo="mediaActions.takeVideo"
        @insertFile="mediaActions.insertFile"
        style="width: 100%; flex-shrink: 0"
        :style="{maxHeight: BAR_MAX_HEIGHT + 'px'}"
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
