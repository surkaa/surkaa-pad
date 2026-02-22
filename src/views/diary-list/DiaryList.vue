<script setup lang="ts">
import {computed, onMounted, ref} from "vue";
import {useRouter} from "vue-router";
import DiaryListHeader from "./DiaryListHeader.vue";
import DiarySummaryCard from "../../components/DiarySummaryCard.vue";
import DiaryListEmpty from "./DiaryListEmpty.vue";
import {commands, DiarySummary} from "../../bindings.ts";

const router = useRouter();
const diaryIds = ref<string[]>([]);
const diarySummaries = ref<Record<string, DiarySummary | null>>({});
const nextToken = ref<string | null>(null);
// 用于判断是否已经完成首次加载，防止一开始数据还没回来就显示“空状态”
const isFirstLoadFinished = ref(false);

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
async function onLoad(index: number, done: (stop?: boolean) => void) {
  try {
    const res = await commands.cmdPageDiaryIds(nextToken.value);
    if (res.status == 'error') {
      console.error('加载日记ID失败:', res.error);
      done(true);
      return;
    }
    const [ids, nt] = res.data;
    console.log(`Page ${index} loaded. IDs:`, ids, 'Next:', nt);
    if (!ids || ids.length === 0) {
      done(true);
      return;
    }

    for (const id of ids) {
      if (!diarySummaries.value[id]) {
        // 初始化占位
        diarySummaries.value[id] = null;
        // 加入渲染列表
        diaryIds.value.push(id);
        // 异步加载摘要
        loadDiarySummer(id).then();
      }
    }

    nextToken.value = nt;

    // 如果 nextToken 为空，说明没有下一页了，否则表示本次加载完成，可以准备下一次
    done(nt === null);
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

onMounted(async () => {
  console.log("DiaryList mounted");
});
</script>

<template>
  <main id="diary-list">
    <DiaryListHeader :stats="diaryStats"/>

    <div class="main-content">
      <section id="list" class="scroll-container" ref="scrollContainer">
        <q-infinite-scroll v-show="diaryIds.length > 0 || !isFirstLoadFinished" @load="onLoad" :offset="250">
          <DiarySummaryCard
              v-for="id in diaryIds"
              :key="id"
              :diary="diarySummaries[id]"
              @click="openDiary(id)"
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

    .list-header {
      margin-bottom: 16px;
      padding: 8px 0;
      border-bottom: 1px solid var(--pad-border-color-100);

      .list-info {
        display: flex;
        justify-content: space-between;
        align-items: center;

        .info-text {
          font-size: 14px;
          font-weight: 500;
          color: var(--pad-text-color-200);
        }

        .sort-icon {
          margin-left: auto;
          margin-right: 4px;
          display: inline-block;
          vertical-align: middle;
          font-size: 10px;
        }

        .sort-indicator {
          font-size: 12px;
          color: var(--pad-text-color-400);
          display: flex;
          align-items: center;
          gap: 4px;
          cursor: pointer;
          text-decoration: underline;
        }
      }
    }

    .diary-list {
      list-style: none;
      padding: 0;
      margin: 0;
      position: relative;
    }
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

// 列表过渡动画
.list-enter-active,
.list-leave-active {
  transition: all var(--pad-transition-base) cubic-bezier(0.4, 0, 0.2, 1);
}

.list-enter-from {
  opacity: 0;
  transform: translateY(20px) scale(0.95);
}

.list-leave-to {
  opacity: 0;
  transform: translateY(-20px) scale(0.95);
}

.list-leave-active {
  position: absolute;
  width: 100%;
  box-sizing: border-box;
  pointer-events: none;
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
