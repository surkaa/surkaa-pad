<script setup lang="ts">
import {computed, onMounted} from "vue";
import DiaryHeader from "./DiaryHeader.vue";
import LiveRichEditor from "../../components/LiveRichEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {formatTimestamp} from "../../utils";
import {useDiaryCore} from "../../composables/useDiaryCore.ts";
import {useRoute} from "vue-router";
import {useQuasar} from "quasar";
import {useEditorUI} from "../../composables/useEditorUI.ts";

const route = useRoute();
const $q = useQuasar();

const initialDiaryId = (route.params.id as string) || "";
const {
  diary, diaryContent, isNew, isInitialLoaded,
  loadDiaryInfo, deleteDiary
} = useDiaryCore(initialDiaryId);

// UI交互
const {
  showMenu, showToolbar, showToolbarPanel, mediaActions,
  setupToolbar,
} = useEditorUI();

const canUndo = computed(() => false);
const canRedo = computed(() => false);

function operate() {
  showMenu.value = true;
}

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
</script>

<template>
  <main>
    <DiaryHeader
        class="header"
        :title="diary?.title"
        @back="$router.back()"
        @info="showDiaryDetail"
        @operate="operate"
        style="width: 100%; flex-shrink: 0"
    />
    <LiveRichEditor
        v-if="diary"
        v-model="diaryContent"
        :diarySummary="diary"
        style="width: 100%; flex: 1"
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
