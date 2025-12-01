<script setup lang="ts">
import {computed, onMounted, ref, watch} from "vue";
import {DiaryManifest} from "../types";
import {useAppStore} from "../stores/app.ts";
import {useRouter} from "vue-router";

const router = useRouter();
const appStore = useAppStore();
const searchTerm = ref('');
const diaries = ref<DiaryManifest[]>([]);
const matchIds = ref<Set<string>>(new Set());
const isSyncing = ref(false); // 新增同步状态Loading

const filteredDiaries = computed<DiaryManifest[]>(() => {
  let result = diaries.value;
  if (matchIds.value.size > 0) {
    result = diaries.value.filter(diary => matchIds.value.has(diary.id));
  }
  // 默认按创建时间倒序排列 (最新的在上面)
  return result.slice().sort((a, b) => b.createdAt - a.createdAt);
});

function loadLocalDiaries() {
  appStore.loadLocalDiaries().then((remoteDiaries) => {
    diaries.value = remoteDiaries;
  });
}

// 绑定到同步按钮
async function syncFromOss() {
  if (isSyncing.value) return;
  isSyncing.value = true;
  try {
    await appStore.syncFromOss();
    // 同步完成后重新加载列表
    loadLocalDiaries();
  } catch (e) {
    console.error("Sync failed", e);
    // 这里可以加一个全局提示，暂时略过
  } finally {
    isSyncing.value = false;
  }
}

// 绑定到列表项点击
function openDiary(diary: DiaryManifest) {
  router.push({
    name: 'DiaryDetail',
    state: {diary}
  });
}

// 绑定到悬浮按钮
function newDiary() {
  router.push({name: 'DiaryDetail'});
}

// 格式化时间辅助函数
function formatDate(timestamp: number) {
  return new Date(timestamp).toLocaleString('zh-CN', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  });
}

onMounted(() => {
  loadLocalDiaries();
  watch(searchTerm, async (term) => {
    console.log(`Searching for term: ${term}`);
    // 如果搜索词为空，清空匹配集，显示所有
    if (!term.trim()) {
      matchIds.value = new Set();
      return;
    }
    const matchIdArr = await appStore.searchWithKeyword(term);
    matchIds.value = new Set(matchIdArr);
  })
});
</script>

<template>
  <main id="diary-list">
    <header class="top-bar">
      <div class="search-box">
        <input
            type="text"
            v-model="searchTerm"
            placeholder="搜索记忆..."
        />
        <i class="icon-search">🔍</i>
      </div>

      <button
          class="sync-btn"
          @click="syncFromOss"
          :disabled="isSyncing"
          title="从云端同步"
      >
        <span v-if="isSyncing" class="spinning">⟳</span>
        <span v-else>☁️</span>
      </button>
    </header>

    <hr/>

    <section id="list" class="scroll-container">
      <transition-group name="list" tag="ul">
        <li
            v-for="diary in filteredDiaries"
            :key="diary.id"
            class="diary-card"
            @click="openDiary(diary)"
        >
          <div class="card-header">
            <span class="date">{{ formatDate(diary.createdAt) }}</span>
            <span v-if="diary.attachments?.length" class="attachment-icon">📎</span>
          </div>
          <p class="preview-content">
            {{ diary.content || '无标题日记' }}
          </p>
        </li>
      </transition-group>

      <div v-if="filteredDiaries.length === 0" class="empty-state">
        <p>这里空空如也 🍂</p>
      </div>
    </section>

    <button class="fab" @click="newDiary" title="写日记">
      +
    </button>
  </main>
</template>

<style scoped lang="scss">
#diary-list {
  position: relative;
  width: 100%;
  height: 100%;
  max-width: 800px; /* 限制最大宽度，提升大屏阅读体验 */
  display: flex;
  flex-direction: column;
  padding: 0 1rem;
  box-sizing: border-box;

  /* 顶部栏布局 */
  .top-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 0 0.5rem 0;
    gap: 10px;

    .search-box {
      position: relative;
      flex-grow: 1;

      input {
        width: 100%;
        padding: 10px 15px 10px 35px; /* 左侧留出图标位置 */
        box-sizing: border-box;
        border: 1px solid var(--pad-border-color-200);
        background-color: var(--pad-bg-color-200);
        color: var(--pad-text-color-200);
        border-radius: 8px;
        outline: none;
        transition: all 0.3s;

        &::placeholder {
          color: var(--pad-text-color-400);
        }

        &:focus {
          border-color: var(--pad-border-color-300);
          background-color: var(--pad-bg-color-100);
          box-shadow: 0 0 0 2px var(--pad-shadow-color-100);
        }
      }

      .icon-search {
        position: absolute;
        left: 10px;
        top: 50%;
        transform: translateY(-50%);
        font-style: normal;
        font-size: 0.9rem;
        opacity: 0.6;
      }
    }

    .sync-btn {
      background: none;
      border: 1px solid var(--pad-border-color-200);
      border-radius: 8px;
      width: 42px;
      height: 42px;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--pad-text-color-300);
      background-color: var(--pad-bg-color-200);
      transition: all 0.2s;

      &:hover:not(:disabled) {
        border-color: var(--pad-primary-color);
        color: var(--pad-primary-color);
        background-color: var(--pad-bg-color-100);
      }

      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      .spinning {
        display: inline-block;
        animation: spin 1s linear infinite;
      }
    }
  }

  /* 列表容器 */
  .scroll-container {
    flex-grow: 1;
    overflow-y: auto;
    padding-bottom: 80px; /* 为 FAB 留出空间 */

    /* 隐藏滚动条但保留功能 */
    &::-webkit-scrollbar {
      width: 4px;
    }

    &::-webkit-scrollbar-thumb {
      background-color: var(--pad-border-color-200);
      border-radius: 4px;
    }

    ul {
      list-style: none;
      padding: 0;
      margin: 0;
    }
  }

  /* 日记卡片样式 */
  .diary-card {
    background-color: var(--pad-bg-color-200);
    border: 1px solid var(--pad-border-color-100);
    border-radius: 8px;
    margin-bottom: 12px;
    padding: 16px;
    cursor: pointer;
    transition: transform 0.2s, box-shadow 0.2s, border-color 0.2s;

    /* 纸张质感阴影 */
    box-shadow: 0 2px 4px var(--pad-shadow-color-100);

    &:hover {
      transform: translateY(-2px);
      box-shadow: 0 4px 12px var(--pad-shadow-color-200);
      border-color: var(--pad-border-color-300);
    }

    .card-header {
      display: flex;
      justify-content: space-between;
      margin-bottom: 8px;

      .date {
        font-size: 0.85rem;
        color: var(--pad-text-color-400);
        font-weight: 500;
      }

      .attachment-icon {
        font-size: 0.85rem;
      }
    }

    .preview-content {
      margin: 0;
      font-size: 1rem;
      color: var(--pad-text-color-200);
      line-height: 1.5;

      /* 多行文本截断 */
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow: hidden;
      text-overflow: ellipsis;
    }
  }

  /* 空状态 */
  .empty-state {
    padding: 40px;
    text-align: center;
    color: var(--pad-text-color-400);
    font-size: 0.9rem;
  }

  /* 悬浮新增按钮 (FAB) */
  .fab {
    position: absolute;
    bottom: 30px;
    right: 20px;
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background-color: var(--pad-primary-color);
    color: #fff; /* 这里的文字颜色固定为白，或者使用 light mode 下的背景色如果 primary 很浅 */
    /* 考虑到 primary 是 C4A484 (木色)，白色文字对比度尚可，但在深色模式下 primary 变浅，可能需要深色字 */
    /* 优化：使用 CSS 变量反转色，或者简单的阴影 */
    border: none;
    font-size: 2rem;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 4px 10px var(--pad-shadow-color-400);
    cursor: pointer;
    transition: transform 0.2s, box-shadow 0.2s;
    z-index: 10;

    &:hover {
      transform: scale(1.05);
      box-shadow: 0 6px 15px var(--pad-shadow-color-500);
    }

    &:active {
      transform: scale(0.95);
    }
  }

  /* 列表过渡动画 */
  .list-enter-active,
  .list-leave-active {
    transition: all 0.4s cubic-bezier(0.25, 0.8, 0.25, 1);
  }

  .list-enter-from,
  .list-leave-to {
    opacity: 0;
    transform: translateY(20px);
  }

  .list-leave-active {
    position: absolute;
    width: 100%; /* 确保离开时宽度不变，防止布局塌陷 */
    box-sizing: border-box; /* 包含 padding */
  }
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>