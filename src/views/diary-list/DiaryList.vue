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

const timeoutStore = useTimeoutStore();
const dataStore = useDataStore();
const {
  diaryIds,
  diarySummaries,
  withAttachments
} = storeToRefs(dataStore);
const {openDiary} = useOpenDiaryDetail();
const configStore = useConfigStore();
const pinnedDiaryIds = configStore.useTauriConfig('pinned_diary_ids');
const nextToken = ref<string | null>(null);
// 用于判断是否已经完成首次加载，防止一开始数据还没回来就显示“空状态”
const isFirstLoadFinished = ref(false);
const isLoading = ref(false);

const scrollContainer = ref<HTMLElement | null>(null);
const {y} = useScroll(scrollContainer, {behavior: 'smooth'})

// 激活状态
const isActivating = ref(true);

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
    console.error(`请求日记 ${id} 摘要失败:`, e);
  }
}

// 无限滚动的核心回调函数
async function onLoad(_index: number, done: (stop?: boolean) => void) {
  // 如果正在加载中，直接跳过，防止由于滚动过快导致的重复请求
  if (isLoading.value) {
    done(false);
    return;
  }

  isLoading.value = true; // 开始加载
  try {
    const [ids, nt] = await api.cmdPageDiaryIds(nextToken.value);

    for (const id of ids) {
      if (diarySummaries.value[id] === undefined) {
        // 初始化占位，供骨架屏使用
        diarySummaries.value[id] = null;
        // 加入渲染列表
        diaryIds.value.push(id);
      }
    }

    if (!nt) {
      // 以是否有 nextToken 来判断是否还有下一页，
      // 后端的listObjectV2返回的是所有对象，
      // 可能存在一整页都是非日记主文件的情况，
      // 所以不能以返回的ID数量来判断
      done(true);
      return;
    }

    nextToken.value = nt;

    // 本次加载完成，可以准备下一次
    done(false);
  } catch (error) {
    console.error('加载失败:', JSON.stringify(error), error);
    done(true);
  } finally {
    isLoading.value = false;
    // 标记首次加载完成
    if (!isFirstLoadFinished.value) {
      isFirstLoadFinished.value = true;
    }
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
        <q-infinite-scroll
            scroll-target="#list"
            v-show="diaryIds.length > 0 || !isFirstLoadFinished"
            @load="onLoad"
            :offset="250"
        >
          <DiarySummaryCard
              v-for="id in sortedDiaryIds"
              :key="id"
              :diary="diarySummaries[id]"
              :pinned="pinnedDiaryIds.includes(id)"
              @click="openDiary(id)"
              @visible="handleCardVisible(id)"
          />

          <template v-slot:loading>
            <div class="row justify-center q-my-md">
              <q-spinner-dots color="primary" size="40px"/>
            </div>
          </template>
        </q-infinite-scroll>

        <div v-if="diaryIds.length === 0 && !isLoading">
          <DiaryListEmpty @create="openDiary()"/>
        </div>
      </section>
    </div>

    <q-page-sticky position="bottom-right" :offset="[24, 38]" class="z-fab">
      <q-btn
          fab
          icon="add"
          label="新建"
          color="primary"
          padding="16px 20px"
          class="pad-fab-gradient"
          @click="openDiary()"
      />
    </q-page-sticky>

    <Teleport v-if="isActivating" defer to="#header-actions">
      <q-btn @click="$router.push({ name: 'DiarySearch' })">搜索</q-btn>
      <q-btn @click="$router.push({ name: 'Settings' })">设置</q-btn>
    </Teleport>
    <Teleport v-if="isActivating" defer to="#footer-content">
      <span>{{ withAttachments }} / {{ diaryIds.length }}</span>
      <span>Time: {{ timeoutStore.remainingStr }}</span>
    </Teleport>
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
