<script setup lang="ts">
import {computed, onDeactivated, onMounted, ref, watch} from "vue";
import {commands, DiarySummary} from "../../bindings.ts";
import {useRoute, useRouter} from "vue-router";
import DiaryHeader from "./DiaryHeader.vue";
import LiveRichEditor from "../../components/LiveRichEditor.vue";
import EditToolbar from "../../components/EditToolbar.vue";
import {useQuasar} from "quasar";
import {formatTimestamp} from "../../utils";
import {platform} from "@tauri-apps/plugin-os";
import {useKeyboardShow} from "../../composables/useKeyboardShow.ts";
import {eventBusEmit} from "../../utils/eventBus.ts";

const $q = useQuasar();
const route = useRoute();
const router = useRouter();

const diaryId = ref<string>("");
const diary = ref<DiarySummary>();
const diaryContent = ref<string>("");

const showMenu = ref(false);
const showToolbar = ref(false);
const showToolbarPanel = ref(false);
const showToolbarAfterMenu = ref(false);
const canUndo = computed(() => false);
const canRedo = computed(() => false);

let saveTimeout: ReturnType<typeof setTimeout> | null = null;
const AUTO_SAVE_DELAY = 1000;
// 标记是否已经完成初次加载，避免将后端的初次赋值误认为用户的输入
const isInitialLoaded = ref(false);

const isNew = computed(() => diaryId.value.trim() === "");

async function loadDiaryInfo(id: string) {
  // 获取日记摘要
  const res = await commands.cmdGetDiarySummary(id);
  if (res.status === 'error') {
    console.error(`加载日记 ${id} 摘要失败:`, res.error);
    return;
  }
  diary.value = res.data;

  // 获取日记内容
  const contentRes = await commands.cmdGetDiaryContent(id);
  if (contentRes.status === 'error') {
    console.error(`加载日记 ${id} 内容失败:`, contentRes.error);
    return;
  }
  diaryContent.value = contentRes.data;

  // 延迟标记加载完成，避免触发首次 watch
  setTimeout(() => { isInitialLoaded.value = true; }, 50);
}

async function saveDiary() {
  if (isNew.value) {
    const res = await commands.cmdSaveDiary(diaryContent.value);
    if (res.status === 'error') {
      $q.notify({
        type: 'negative',
        message: `保存日记失败: ${res.error}`
      });
      return;
    }
    const [summary, content] = res.data;
    diaryId.value = summary.id;
    diary.value = summary;
    diaryContent.value = content;
    $q.notify({
      type: 'positive',
      message: '日记已自动创建'
    });
    eventBusEmit('diary-changed', {
      type: 'created',
      summary,
    });
    return;
  }
  // 已存在的日记，执行更新
  const res = await commands.cmdUpdateDiaryContentOnly(diaryId.value, diaryContent.value);
  if (res.status === 'error') {
    $q.notify({
      type: 'negative',
      message: `保存日记失败: ${res.error}`
    });
    return;
  }
  const summary = res.data;
  diary.value = summary;
  eventBusEmit('diary-changed', {
    type: 'updated',
    summary,
  });
}

function operate() {
  showMenu.value = true;
}

function deleteDiary() {
  if (!diaryId.value) {
    console.log('没有日记ID，无法删除');
    return;
  }
  $q.dialog({
    title: '确认删除',
    message: '确定要删除这篇日记吗？此操作无法撤销。',
    ok: {
      label: '删除',
      color: 'negative',
      flat: true
    },
    cancel: {
      label: '取消',
      color: 'primary',
      flat: true
    }
  }).onOk(async () => {
    const res = await commands.cmdDeleteDiary(diaryId.value);
    if (res.status === 'error') {
      $q.notify({
        type: 'negative',
        message: `删除日记失败: ${res.error}`
      });
    } else {
      $q.notify({
        type: 'positive',
        message: '日记已删除'
      });
      eventBusEmit('diary-changed', {
        type: 'deleted',
        id: diaryId.value
      });
      router.back();
    }
  });
  showMenu.value = false;
}

function showDiaryDetail() {
  if (!diary.value) {
    $q.notify({
      message: '日记信息未加载'
    });
    return;
  }
  const {title, created, updated, attachments} = diary.value;
  $q.dialog({
    title,
    message: `创建时间：${formatTimestamp(created)}<br>更新时间：${formatTimestamp(updated)}<br>附件数量：${attachments.length}`,
    html: true,
    ok: {
      label: '关闭',
      color: 'primary',
      flat: true
    },
  });
}

function setupToolbar() {
  const p = platform();
  if (p == 'android') {
    // 目前这个键盘只测试了安卓手机
    useKeyboardShow(showToolbar);
  } else {
    // 其他平台默认显示工具栏
    showToolbar.value = true;
  }
}

watch(showMenu, (newVal) => {
  if (newVal) {
    // 打开菜单时隐藏工具栏
    showToolbarAfterMenu.value = showToolbar.value;
    showToolbar.value = false;
    showToolbarPanel.value = false;
  } else {
    // 关闭菜单时恢复工具栏状态
    showToolbar.value = showToolbarAfterMenu.value;
  }
});

// 监听日记内容的变化
watch(diaryContent, (newValue, oldValue) => {
  // 如果还没加载完，或者值根本没变，则不触发保存
  if (!isInitialLoaded.value || newValue === oldValue) return;

  // 清除上一次的定时器（防抖）
  if (saveTimeout) clearTimeout(saveTimeout);

  // 开启新的定时器
  saveTimeout = setTimeout(saveDiary, AUTO_SAVE_DELAY);
});

onMounted(async () => {
  diaryId.value = route.params.id as string || "";
  if (!isNew.value) {
    await loadDiaryInfo(diaryId.value);
  } else {
    // 新建日记，直接标记加载完成，允许保存
    isInitialLoaded.value = true;
  }
  setupToolbar();
});

// 组件卸载时，如果还有没保存的，强制保存一次
onDeactivated(() => {
  if (saveTimeout) {
    clearTimeout(saveTimeout);
    saveDiary();
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
        @operate="operate"
        style="width: 100%; flex-shrink: 0"
    />
    <LiveRichEditor
        v-model="diaryContent"
        style="width: 100%; flex: 1"
    />
    <EditToolbar
        :view="showToolbar || showToolbarPanel"
        :panelOpen="showToolbarPanel"
        :undo="canUndo"
        :redo="canRedo"
        @additionalAction="showToolbarPanel = !showToolbarPanel"
        v-click-outside="() => showToolbarPanel = false"
        style="width: 100%; flex-shrink: 0"
    />

    <q-dialog v-model="showMenu" position="bottom">
      <q-card class="action-sheet-card">
        <q-list padding class="text-center">
          <q-item clickable v-ripple @click="">
            <q-item-section>操作1</q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="deleteDiary">
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
