<script setup lang="ts">
import {computed, nextTick, onActivated, onDeactivated, ref} from "vue";
import {useRouter} from "vue-router";
import DiarySummaryCard from "../../components/DiarySummaryCard.vue";
import DiaryListEmpty from "./DiaryListEmpty.vue";
import {commands} from "../../bindings.ts";
import {useAppStore} from "../../stores/app.ts";
import {useScroll, useTimestamp} from "@vueuse/core";
import {useDataStore} from "../../stores/data.ts";
import {storeToRefs} from "pinia";

const {getEndTime} = useAppStore();
const router = useRouter();
const {
  diaryIds,
  diarySummaries,
  currentId,
  withAttachments,
} = storeToRefs(useDataStore());
const nextToken = ref<string | null>(null);
// 用于判断是否已经完成首次加载，防止一开始数据还没回来就显示“空状态”
const isFirstLoadFinished = ref(false);

const scrollContainer = ref<HTMLElement | null>(null);
const {y} = useScroll(scrollContainer, {behavior: 'smooth'})

// 倒计时
const now = useTimestamp({offset: 0, interval: 1000})

// 计算剩余时间字符串，格式为 MM:SS
const remainingStr = computed(() => {
  const diff = new Date(getEndTime).getTime() - now.value
  const ms = Math.max(0, diff);
  const seconds = Math.floor(ms / 1000) % 60;
  const minutes = Math.floor(ms / (1000 * 60)) % 60;
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
});

// 激活状态
const isActivating = ref(true);

// 获取单个日记的摘要
async function loadDiarySummer(id: string) {
  try {
    const res = await commands.cmdGetDiarySummary(id);
    if (res.status === 'error') {
      console.error(`加载日记 ${id} 摘要失败:`, res.error);
      return;
    }
    diarySummaries.value[id] = res.data;
  } catch (e) {
    console.error(`请求日记 ${id} 摘要失败:`, e);
  }
}

// 无限滚动的核心回调函数
async function onLoad(_index: number, done: (stop?: boolean) => void) {
  try {
    const res = await commands.cmdPageDiaryIds(nextToken.value);
    if (res.status == 'error') {
      console.error('加载日记ID失败:', res.error);
      done(true);
      return;
    }
    const [ids, nt] = res.data;

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
    console.error('加载失败:', error);
    done(true);
  } finally {
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

// 绑定到列表项点击
function openDiary(id?: string) {
  if (!id) {
    // 新建日记
    currentId.value = "";
    router.push({name: 'DiaryDetail'});
    return;
  }
  // 打开已有日记
  currentId.value = id;
  router.push({name: 'DiaryDetail'});
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
              v-for="id in diaryIds"
              :key="id"
              :diary="diarySummaries[id]"
              @click="openDiary(id)"
              @visible="handleCardVisible(id)"
          />
        </q-infinite-scroll>

        <div v-if="diaryIds.length === 0">
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
      <span>Time: {{ remainingStr }}</span>
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
