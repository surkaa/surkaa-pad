<script setup lang="ts">
import {computed, nextTick, onActivated, onDeactivated, ref, watch} from "vue";
import DiarySummaryCard from "../../components/DiarySummaryCard.vue";
import DiaryListEmpty from "./DiaryListEmpty.vue";
import {useScroll} from "@vueuse/core";
import {useDataStore} from "../../stores/data.ts";
import {storeToRefs} from "pinia";
import {useTimeoutStore} from "../../stores/timeout.ts";
import {useOpenDiaryDetail} from "../../composables/useOpenDiaryDetail.ts";
import api from "../../utils/api.ts";
import {useConfigStore} from "../../stores/config.ts";
import {useQuasar} from "quasar";
import {
  formatError,
  isNewerDiaryVersionError,
  NEWER_DIARY_VERSION_MESSAGE
} from "../../utils/formatError.ts";
import {useRouter} from 'vue-router';
import {useDiaryListShortcuts} from '../../composables/useDiaryListShortcuts';
import {formatEditorShortcut} from '../../utils/editorShortcuts';
import {platform} from '@tauri-apps/plugin-os';
import {useAndroidShareStore} from '../../stores/androidShare';

const $q = useQuasar();
const router = useRouter();
const isWindows = platform() === 'windows';
const androidShareStore = useAndroidShareStore();
const {selectingTarget} = storeToRefs(androidShareStore);
const timeoutStore = useTimeoutStore();
const dataStore = useDataStore();
const {
  diaryIds,
  diarySummaries,
  diaryListRevision,
  withAttachments
} = storeToRefs(dataStore);
const {openDiary} = useOpenDiaryDetail();
const configStore = useConfigStore();
const pinnedDiaryIds = configStore.useTauriConfig('pinned_diary_ids');
const diaryListShortcuts = configStore.useTauriConfig('windows_diary_list_shortcuts');
const nextToken = ref<string | null>(null);
// 用于判断是否已经完成首次加载，防止一开始数据还没回来就显示“空状态”
const isFirstLoadFinished = ref(false);
const isLoading = ref(false);
const isOpeningDiary = ref(false);
const localDraftIds = ref<string[]>([]);
const loadedDiaryListRevision = ref(diaryListRevision.value);
interface InfiniteScrollHandle {
  reset: () => void;
  resume: () => void;
  stop: () => void;
}
const infiniteScrollRef = ref<InfiniteScrollHandle | null>(null);
const diaryListRenderRevision = ref(0);
let diaryListLoadGeneration = 0;

const scrollContainer = ref<HTMLElement | null>(null);
const {y} = useScroll(scrollContainer, {behavior: 'smooth'})

// 激活状态
const isActivating = ref(true);

function openSearch() {
  if (isOpeningDiary.value) return;
  void router.push({name: 'DiarySearch'});
}

function openAiAssistant() {
  if (isOpeningDiary.value) return;
  void router.push({name: 'AiAssistant'});
}

function openSettings() {
  if (isOpeningDiary.value) return;
  void router.push({name: 'Settings'});
}

async function openNewDiary() {
  if (isOpeningDiary.value) return;
  isOpeningDiary.value = true;
  try {
    await openDiary();
  } finally {
    isOpeningDiary.value = false;
  }
}

async function handleDiaryClick(id: string) {
  if (isOpeningDiary.value) return;
  isOpeningDiary.value = true;
  try {
    if (selectingTarget.value) {
      const batchId = androidShareStore.selectTarget(id);
      dataStore.currentId = id;
      await router.push({
        name: 'DiaryDetail',
        query: {shareImport: batchId},
      });
      return;
    }
    await openDiary(id);
  } finally {
    isOpeningDiary.value = false;
  }
}

useDiaryListShortcuts(diaryListShortcuts, {
  createDiary: () => void openNewDiary(),
  aiAssistant: openAiAssistant,
  search: openSearch,
  settings: openSettings,
});

const sortedDiaryIds = computed(() => {
  const pinned: string[] = [];
  const unpinned: string[] = [];

  // 遍历当前的 diaryIds，按是否置顶分发到两个数组中
  diaryIds.value.forEach(id => {
    if (pinnedDiaryIds.value.includes(id)) {
      pinned.push(id);
    } else {
      unpinned.push(id);
    }
  });

  // 返回拼接后的数组：置顶在前，未置顶在后
  return [...pinned, ...unpinned];
});

watch(pinnedDiaryIds, (newPinnedIds) => {
  if (!newPinnedIds) return;

  newPinnedIds.forEach(id => {
    if (!diaryIds.value.includes(id)) {
      if (diarySummaries.value[id] === undefined) {
        // 初始化占位，供骨架屏使用
        diarySummaries.value[id] = null;
      }
      // 加入底层列表，sortedDiaryIds 会自动将它提到最前面
      diaryIds.value.push(id);
    }
  });
}, { immediate: true });

// 获取单个日记的摘要
async function loadDiarySummer(id: string) {
  try {
    const summary = await api.cmdGetDiarySummary(id);
    dataStore.insertNewDiary(summary);
  } catch (e) {
    console.error(`请求日记 ${id} 摘要失败:`, JSON.stringify(e), e);
    $q.notify({
      type: isNewerDiaryVersionError(e) ? 'warning' : 'negative',
      message: isNewerDiaryVersionError(e)
          ? NEWER_DIARY_VERSION_MESSAGE
          : `加载日记摘要失败: ${formatError(e)}`
    });
  }
}

async function loadDiaryPage(generation: number) {
  isLoading.value = true;
  try {
    const [ids, nt] = await api.cmdPageDiaryIds(nextToken.value);
    if (generation !== diaryListLoadGeneration) return true;

    for (const id of ids) {
      if (diarySummaries.value[id] === undefined) {
        // 初始化占位，供骨架屏使用
        diarySummaries.value[id] = null;
        // 加入渲染列表
        diaryIds.value.push(id);
      }
    }

    nextToken.value = nt;
    // 以是否有 nextToken 来判断是否还有下一页；对象列表的一整页可能没有日记主文件。
    return !nt;
  } finally {
    if (generation === diaryListLoadGeneration) {
      isLoading.value = false;
      isFirstLoadFinished.value = true;
    }
  }
}

// 无限滚动的核心回调函数
async function onLoad(_index: number, done: (stop?: boolean) => void) {
  // 如果正在加载中，直接跳过，防止由于滚动过快导致的重复请求
  if (isLoading.value) {
    done(false);
    return;
  }

  const generation = diaryListLoadGeneration;
  try {
    done(await loadDiaryPage(generation));
  } catch (error) {
    console.error('加载失败:', JSON.stringify(error), error);
    done(true);
  }
}

function resetDiaryListContent() {
  const pinnedIds = [...new Set(pinnedDiaryIds.value)];
  // 置顶卡片可能已解除过 IntersectionObserver。重建卡片后，刷新产生的骨架态
  // 会重新注册可见性监听，只加载当前视口附近的摘要。
  diaryListRenderRevision.value += 1;
  diaryIds.value = pinnedIds;
  diarySummaries.value = Object.fromEntries(pinnedIds.map(id => [id, null]));
  nextToken.value = null;
  isFirstLoadFinished.value = false;
}

async function refreshDiaryList(done: () => void) {
  const generation = ++diaryListLoadGeneration;
  infiniteScrollRef.value?.stop();
  isLoading.value = false;
  resetDiaryListContent();

  let stopInfiniteScroll = true;
  try {
    stopInfiniteScroll = await loadDiaryPage(generation);
  } catch (error) {
    console.error('刷新日记列表失败:', JSON.stringify(error), error);
    $q.notify({
      type: 'negative',
      message: `刷新日记列表失败: ${formatError(error)}`,
    });
  } finally {
    await nextTick();
    if (generation === diaryListLoadGeneration) {
      infiniteScrollRef.value?.reset();
      if (!stopInfiniteScroll) infiniteScrollRef.value?.resume();
    }
    done();
  }
}

// 供子组件触发的视口加载回调
function handleCardVisible(id: string) {
  // 只有当数据为 null (占位态) 时才发请求
  if (diarySummaries.value[id] === null) {
    loadDiarySummer(id).then();
  }
}

defineOptions({name: 'DiaryList'});

onActivated(async () => {
  isActivating.value = true;
  try { localDraftIds.value = await api.cmdListDiaryDraftIds(); }
  catch (error) { $q.notify({type: 'negative', message: `读取本机草稿列表失败：${formatError(error)}`}); }
  if (loadedDiaryListRevision.value !== diaryListRevision.value) {
    loadedDiaryListRevision.value = diaryListRevision.value;
    diaryListLoadGeneration += 1;
    infiniteScrollRef.value?.stop();
    isLoading.value = false;
    resetDiaryListContent();
    await nextTick();
    infiniteScrollRef.value?.reset();
    infiniteScrollRef.value?.resume();
  }
  // 等待 DOM 渲染完毕
  await nextTick();
  if (scrollContainer.value) {
    scrollContainer.value.scrollTop = y.value;
  }
});

onDeactivated(() => {
  isActivating.value = false;
});
</script>

<template>
  <div id="diary-list">
    <div class="main-content">
      <section id="list" class="scroll-container" ref="scrollContainer">
        <q-pull-to-refresh
            class="diary-list-refresh"
            scroll-target="#list"
            color="primary"
            :no-mouse="isWindows"
            :disable="isOpeningDiary"
            @refresh="refreshDiaryList"
        >
          <q-banner v-if="selectingTarget" rounded class="share-target-banner q-mb-md">
            <template #avatar><q-icon name="ios_share"/></template>
            请选择要添加分享内容的日记
            <template #action>
              <q-btn flat label="取消" @click="androidShareStore.cancelTargetSelection()"/>
            </template>
          </q-banner>
          <q-banner v-if="localDraftIds.length && !selectingTarget" rounded class="share-target-banner q-mb-md">
            <template #avatar><q-icon name="save"/></template>
            本机有未处理的加密草稿，点击打开后可恢复或复制。
            <div class="q-mt-sm">
              <q-btn v-for="id in localDraftIds" :key="id" flat color="primary" :label="`草稿 ${id}`"
                     :disable="isOpeningDiary" @click="handleDiaryClick(id)"/>
            </div>
          </q-banner>
          <q-infinite-scroll
              ref="infiniteScrollRef"
              scroll-target="#list"
              v-show="diaryIds.length > 0 || !isFirstLoadFinished"
              @load="onLoad"
              :offset="250"
          >
            <DiarySummaryCard
                v-for="id in sortedDiaryIds"
                :key="`${diaryListRenderRevision}:${id}`"
                :diary="diarySummaries[id]"
                :pinned="pinnedDiaryIds.includes(id)"
                @click="handleDiaryClick(id)"
                @visible="handleCardVisible(id)"
            />

            <template v-slot:loading>
              <div class="row justify-center q-my-md">
                <q-spinner-dots color="primary" size="40px"/>
              </div>
            </template>
          </q-infinite-scroll>

          <div v-if="diaryIds.length === 0 && !isLoading">
            <DiaryListEmpty @create="openNewDiary()"/>
          </div>
        </q-pull-to-refresh>
      </section>
    </div>

    <q-page-sticky v-if="!selectingTarget" position="bottom-right" :offset="[24, 38]" class="z-fab">
      <q-btn
          fab
          icon="add"
          label="新建"
          color="primary"
          padding="16px 20px"
          class="pad-fab-gradient"
          :disable="isOpeningDiary"
          @click="openNewDiary()"
      >
        <q-tooltip v-if="isWindows">
          新建日记（{{ formatEditorShortcut(diaryListShortcuts.createDiary) }}）
        </q-tooltip>
      </q-btn>
    </q-page-sticky>

    <Teleport v-if="isActivating && !selectingTarget" defer to="#header-actions">
      <q-btn flat round dense icon="auto_awesome" aria-label="AI 助手"
             :disable="isOpeningDiary" @click="openAiAssistant">
        <q-tooltip v-if="isWindows">
          AI 助手（{{ formatEditorShortcut(diaryListShortcuts.aiAssistant) }}）
        </q-tooltip>
      </q-btn>
      <q-btn flat round dense icon="search" aria-label="搜索日记" class="q-ml-md"
             :disable="isOpeningDiary" @click="openSearch">
        <q-tooltip v-if="isWindows">搜索（{{ formatEditorShortcut(diaryListShortcuts.search) }}）</q-tooltip>
      </q-btn>
      <q-btn flat round dense icon="settings" aria-label="设置" class="q-ml-md"
             :disable="isOpeningDiary" @click="openSettings">
        <q-tooltip v-if="isWindows">设置（{{ formatEditorShortcut(diaryListShortcuts.settings) }}）</q-tooltip>
      </q-btn>
    </Teleport>
    <Teleport v-if="isActivating" defer to="#footer-content">
      <span>日记 {{ diaryIds.length }} · 含附件 {{ withAttachments }}</span>
      <span>自动关闭 {{ timeoutStore.remainingStr }}</span>
    </Teleport>
    <q-inner-loading
      :showing="isOpeningDiary"
      color="primary"
      label="正在打开日记…"
      class="diary-opening-overlay"
    />
  </div>
</template>

<style scoped lang="scss">
#diary-list {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  background-color: var(--pad-bg-color-100);
  font-family: var(--pad-font-family), serif;
  position: relative;

  .main-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0;
  }

  .scroll-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 24px 24px 100px 24px;
  }

  .diary-list-refresh {
    min-height: 100%;
  }

  .diary-opening-overlay {
    z-index: 30;
    color: var(--pad-text-color-200);
    background: color-mix(in srgb, var(--pad-bg-color-100) 88%, transparent);
  }

  .share-target-banner {
    border: 1px solid var(--pad-border-color-200);
    background: var(--pad-bg-color-200);
    color: var(--pad-text-color-100);
  }

  .pad-fab-gradient {
    background: var(--pad-primary-gradient) !important;
    color: var(--pad-text-color-light);
    border-radius: var(--pad-radius-xl);
    font-weight: 600;
    box-shadow: var(--pad-shadow-lg);
    transition: all var(--pad-transition-base);

    &:hover {
      transform: translateY(-3px);
      box-shadow: var(--pad-shadow-xl);
    }

    :deep(.q-btn__content) {
      letter-spacing: 0.5px;
    }
  }
}

// 响应式设计
@media (max-width: 512px) {
  .pad-fab-gradient {
    :deep(.q-btn__content .q-anchor--skip) {
      display: none;
    }

    :deep(.q-page-sticky) {
      right: 16px !important;
      bottom: 16px !important;
    }
  }
}
</style>
