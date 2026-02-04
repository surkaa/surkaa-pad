<script setup lang="ts">
import {computed, nextTick, onMounted, onUnmounted, ref, toRaw, watch, type WatchHandle} from "vue";
import {useAppStore} from "../../stores/app.ts";
import {onBeforeRouteLeave, useRouter} from "vue-router";
import {showToast} from "../../utils";
import {debounce} from "../../utils";
import DiaryListHeader from "./DiaryListHeader.vue";
import DiaryListActionBar from "./DiaryListActionBar.vue";
import DiaryCard from "./DiaryCard.vue";
import DiaryListEmpty from "./DiaryListEmpty.vue";
import {commands, DiaryManifest} from "../../bindings.ts";

type OrderBy = 'created' | 'updated';

const router = useRouter();
const appStore = useAppStore();
const diaries = ref<DiaryManifest[]>([]);
const matchIds = ref<Set<string>>(new Set());
const isSyncing = ref(false); // 新增同步状态Loading
const watcher = ref<WatchHandle | null>(null);
const scrollContainer = ref<HTMLElement | null>(null);
const filteredDiaries = computed<DiaryManifest[]>(() => {
  if (matchIds.value.size === 0 && appStore.keyword.trim() !== '') {
    // 有搜索词但无匹配，返回空列表
    return [];
  }
  return diaries.value.filter(diary => {
    // 如果有搜索词，则只显示匹配的日记
    if (appStore.keyword.trim() !== '') {
      return matchIds.value.has(diary.id);
    }
    return true; // 无搜索词，显示所有
  }).sort((a, b) => {
    if (orderBy.value === 'created') {
      return b.created - a.created;
    } else {
      return b.updated - a.updated;
    }
  });
});
const orderBy = ref<OrderBy>('created');
// 防抖搜索函数
const debouncedSearch = debounce((term: string) => {
  performSearch(term);
}, 300);

// 日记统计信息
const diaryStats = computed(() => {
  const total = diaries.value.length;
  const filtered = filteredDiaries.value.length;
  const withAttachments = diaries.value.filter(d => d.attachments?.length > 0).length;
  const lastUpdated = diaries.value.length > 0
      ? Math.max(...diaries.value.map(d => d.updated))
      : 0;

  return {
    total,
    filtered,
    withAttachments,
    lastUpdated,
    hasSearch: appStore.keyword.trim() !== '',
    searchCount: matchIds.value.size
  };
});

async function performSearch(term: string) {
  const matchIdArr = await appStore.searchWithKeyword(term);
  matchIds.value = new Set(matchIdArr);
  showToast('找到 ' + matchIdArr.length + ' 条相关日记', 'success', 1000, {
    position: 'top-center',
  });
}

function loadLocalDiaries() {
  commands.listLocalDiaries().then(res => {
    if (res.status == "error") {
      showToast('加载本地日记失败: ' + res.error, 'error');
      return;
    }
    diaries.value = res.data;
    // 恢复滚动位置
    if (appStore.savedScrollPosition > 0 && scrollContainer.value) {
      // 使用 nextTick 确保列表渲染完毕
      nextTick(() => {
        scrollContainer.value!.scrollTop = appStore.savedScrollPosition;
        console.log('恢复列表滚动位置:', appStore.savedScrollPosition);
        // 恢复后清零，防止在其他情况下（如手动刷新）错误地应用
        appStore.savedScrollPosition = 0;
      });
    }
  });
}

// 绑定到同步按钮
async function syncFromOss(uuid?: string): Promise<DiaryManifest | null> {
  return new Promise(resolve => {
    if (isSyncing.value) return resolve(null); // 防止重复点击
    isSyncing.value = true;
    commands.syncFromOss(uuid ? uuid : null).then(res => {
      if (res.status == "error") {
        showToast('同步失败: ' + res.error, 'error');
        return resolve(null);
      }
      const diary = res.data;
      // 同步完成后重新加载列表
      loadLocalDiaries();
      resolve(diary);
    }).catch(e => {
      console.error("同步失败：", e);
      resolve(null);
    }).finally(() => {
      isSyncing.value = false;
    });
  })
}

// 绑定到列表项点击
function openDiary(diary?: DiaryManifest) {
  if (!diary) {
    // 新建日记
    router.push({name: 'DiaryDetail'});
    return;
  }
  // 先同步单个日记的最新内容
  syncFromOss(diary.id).then(newDiary => {
    if (!newDiary) {
      // 这有bug
      showToast('无法打开日记', 'error');
      return;
    }
    if (newDiary.updated != diary.updated) {
      showToast('已同步改日记最新版', 'info', 1000);
    }
    router.push({
      name: 'DiaryDetail',
      state: {diary: toRaw(newDiary)}
    });
  }).then(() => watcher.value && watcher.value.pause());
}

onBeforeRouteLeave((to, _from, next) => {
  // 只有当目标路由是详情页时才保存（可选，但更精确）
  if (scrollContainer.value && to.name === 'DiaryDetail') {
    appStore.savedScrollPosition = scrollContainer.value.scrollTop;
    console.log('保存列表滚动位置:', appStore.savedScrollPosition);
  }
  next();
});

onMounted(() => {
  console.log("DiaryList mounted");
  syncFromOss(); // 现在每次都会同步日记，下载量比较大，可能确实需要增加一个缓存，但是需要封装好
  if (watcher.value) {
    watcher.value.resume();
    return;
  }
  watcher.value = watch(() => appStore.keyword, async (term) => {
    // 立即清空搜索（不等待防抖）
    if (!term.trim()) {
      matchIds.value = new Set();
      debouncedSearch.cancel(); // 取消待执行的搜索
      return;
    }

    // 使用防抖搜索
    debouncedSearch(term);
  }, {
    immediate: true
  });
});

onUnmounted(() => {
  console.log('DiaryList unmounted');
  if (watcher.value) {
    watcher.value.stop();
  }
  debouncedSearch.cancel();
});
</script>

<template>
  <main id="diary-list">
    <DiaryListHeader :stats="diaryStats"/>

    <div class="main-content">
      <DiaryListActionBar
          v-model:keyword="appStore.keyword"
          :is-syncing="isSyncing"
          @sync="syncFromOss()"
      />

      <section id="list" class="scroll-container" ref="scrollContainer">
        <div class="list-header" v-if="filteredDiaries.length > 0">
          <div class="list-info">
            <span class="info-text">
              {{ diaryStats.hasSearch ? '搜索到' : '共' }} {{ filteredDiaries.length }} 篇日记
            </span>
            <svg viewBox="0 0 24 24" width="14" height="14" class="sort-icon" v-if="orderBy === 'created'">
              <path
                  d="M20 3h-1V1h-2v2H7V1H5v2H4c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 18H4V8h16v13z"/>
            </svg>
            <span class="sort-icon" v-else>🕚</span>
            <span class="sort-indicator" @click="orderBy = (orderBy === 'created' ? 'updated' : 'created')">
              按{{ orderBy === 'created' ? '创建' : '更新' }}时间倒排
            </span>
          </div>
        </div>

        <transition-group name="list" tag="ul" class="diary-list">
          <DiaryCard
              v-for="diary in filteredDiaries"
              :key="diary.id"
              :diary="diary"
              @click="openDiary(diary)"
          />
        </transition-group>

        <div v-if="filteredDiaries.length === 0">
          <DiaryListEmpty
              :is-syncing="isSyncing"
              :has-search="diaryStats.hasSearch"
              @create="openDiary(undefined)"
          />
        </div>
      </section>
    </div>

    <button
        class="fab"
        @click="openDiary(undefined)"
        :title="diaryStats.hasSearch ? '新建日记' : '写新日记'"
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
