<script setup lang="ts">
import {computed, nextTick, onActivated, ref} from "vue";
import {useRouter} from "vue-router";
import DiaryListHeader from "./DiaryListHeader.vue";
import DiarySummaryCard from "../../components/DiarySummaryCard.vue";
import DiaryListEmpty from "./DiaryListEmpty.vue";
import {commands, DiarySummary} from "../../bindings.ts";
import {eventBusOn} from "../../utils/eventBus.ts";

const router = useRouter();
const diaryIds = ref<string[]>([]);
const diarySummaries = ref<Record<string, DiarySummary | null>>({});
const nextToken = ref<string | null>(null);
// 用于判断是否已经完成首次加载，防止一开始数据还没回来就显示“空状态”
const isFirstLoadFinished = ref(false);

// 用于记录滚动位置，保持在列表页和详情页切换时的滚动状态
const savedScrollTop = ref(0);
const scrollContainer = ref<HTMLElement | null>(null);

// 日记统计信息
const diaryStats = computed(() => {
  const total = diaryIds.value.length;
  const withAttachments = diarySummaries.value
      ? Object.values(diarySummaries.value).filter(s => s && s.attachments.length).length
      : 0;

  return {
    total,
    withAttachments,
  };
});

function handleScroll(e: Event) {
  const target = e.target as HTMLElement;
  savedScrollTop.value = target.scrollTop;
}

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
    if (!nt) {
      // 以是否有 nextToken 来判断是否还有下一页，
      // 后端的listObjectV2返回的是所有对象，
      // 可能存在一整页都是非日记主文件的情况，
      // 所以不能以返回的ID数量来判断
      done(true);
      return;
    }

    for (const id of ids) {
      if (diarySummaries.value[id] === undefined) {
        // 初始化占位，供骨架屏使用
        diarySummaries.value[id] = null;
        // 加入渲染列表
        diaryIds.value.push(id);
      }
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
    router.push({name: 'DiaryDetail'});
    return;
  }
  // 打开已有日记
  router.push({name: 'DiaryDetail', params: {id}});
}

defineOptions({
  name: 'DiaryList'
});

onActivated(async () => {
  // 等待 DOM 渲染完毕
  await nextTick();
  if (scrollContainer.value) {
    scrollContainer.value.scrollTop = savedScrollTop.value;
  }
  eventBusOn('diary-changed', async payload => {
    switch (payload.type) {
      case 'created':
        diaryIds.value.unshift(payload.summary.id);
        diarySummaries.value[payload.summary.id] = payload.summary;
        break;
      case 'updated':
        const old = diarySummaries.value[payload.summary.id];
        if (old && old !== payload.summary) {
          diarySummaries.value[payload.summary.id] = payload.summary;
        }
        break;
      case 'deleted':
        const index = diaryIds.value.indexOf(payload.id);
        if (index !== -1) {
          diaryIds.value.splice(index, 1);
          delete diarySummaries.value[payload.id];
        }
        break;
    }
  });
});
</script>

<template>
  <main id="diary-list">
    <DiaryListHeader :stats="diaryStats"/>

    <div class="main-content">
      <section id="list" class="scroll-container" ref="scrollContainer" @scroll="handleScroll">
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

    <button
        class="fab"
        @click="openDiary()"
        title="新建日记"
    >
      <span class="fab-icon">+</span>
      <span class="fab-text">新建</span>
    </button>
  </main>
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
    padding: 0 24px 100px 24px;
  }

  .fab {
    position: fixed;
    bottom: 24px;
    right: 24px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 20px;
    background: var(--pad-primary-gradient);
    color: var(--pad-text-color-light);
    border: none;
    border-radius: var(--pad-radius-xl);
    font-size: 15px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: var(--pad-shadow-lg);
    transition: all var(--pad-transition-base);
    z-index: 100;
    min-width: 100px;

    &:hover {
      transform: translateY(-3px);
      box-shadow: var(--pad-shadow-xl);
    }

    &:active {
      transform: translateY(-1px);
    }

    .fab-icon {
      font-size: 20px;
      font-weight: 400;
    }

    .fab-text {
      letter-spacing: 0.5px;
    }
  }
}

// 响应式设计
@media (max-width: 512px) {
  #diary-list {
    .fab {
      bottom: 16px;
      right: 16px;
      padding: 12px 16px;
      min-width: auto;

      .fab-text {
        display: none;
      }
    }
  }
}

@media (min-width: 513px) and (max-width: 768px) {
  #diary-list {
    .main-content {
      padding: 0 20px;
    }
  }
}
</style>
