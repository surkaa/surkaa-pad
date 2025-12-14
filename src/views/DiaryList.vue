<script setup lang="ts">
import {computed, nextTick, onMounted, onUnmounted, ref, toRaw, watch, type WatchHandle} from "vue";
import {AttachmentMeta, DiaryManifest} from "../types";
import {useAppStore} from "../stores/app.ts";
import {onBeforeRouteLeave, useRouter} from "vue-router";
import {formatBytes, formatTimestamp, getCurEmoji} from "../utils";
import {showToast} from "../utils";
import {invoke} from "@tauri-apps/api/core";
import {debounce} from "../utils";

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
  }).sort((a, b) => b.created - a.created); // 按创建时间降序排列
});
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

// 格式化附件信息
function getAttachmentInfo(attachments: AttachmentMeta[]) {
  if (!attachments || attachments.length === 0) return null;

  const totalSize = attachments.reduce((sum, att) => sum + (att.size || 0), 0);
  const imageCount = attachments.filter(att => att.mimetype.includes('image')).length;
  const otherCount = attachments.length - imageCount;

  return {
    count: attachments.length,
    totalSize,
    imageCount,
    otherCount
  };
}

async function performSearch(term: string) {
  const matchIdArr = await appStore.searchWithKeyword(term);
  matchIds.value = new Set(matchIdArr);
  showToast('找到 ' + matchIdArr.length + ' 条相关日记', 'success', 1000, {
    position: 'top-center',
  });
}

function loadLocalDiaries() {
  invoke<DiaryManifest[]>('list_local_diaries').then(remoteDiaries => {
    diaries.value = remoteDiaries;
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
    invoke<DiaryManifest | null>('sync_from_oss', {uuid}).then(diary => {
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
  loadLocalDiaries();
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
    <!-- 顶部栏 -->
    <header class="app-header">
      <div class="header-content">
        <div class="logo-section">
          <h1 class="app-title">
            <img alt="app-logo" class="app-logo" src="../../public/app-icon.png"/>
            SurKaa Pad
          </h1>
        </div>

        <div class="stats-section" v-if="!diaryStats.hasSearch">
          <div class="stat-item">
            <span class="stat-icon">📚</span>
            <span class="stat-value">{{ diaryStats.total }}</span>
            <span class="stat-label">篇日记</span>
          </div>
          <div class="stat-item" v-if="diaryStats.withAttachments > 0">
            <span class="stat-icon">📎</span>
            <span class="stat-value">{{ diaryStats.withAttachments }}</span>
            <span class="stat-label">含附件</span>
          </div>
        </div>
        <div class="stats-section" v-else>
          <div class="stat-item search-stat">
            <span class="stat-icon">🔍</span>
            <span class="stat-value">{{ diaryStats.searchCount }}</span>
            <span class="stat-label">条结果</span>
          </div>
        </div>
      </div>
    </header>

    <!-- 主内容区域 -->
    <div class="main-content">
      <!-- 搜索和操作栏 -->
      <div class="action-bar">
        <div class="search-container">
          <div class="search-box">
            <input
                id="search-input"
                type="text"
                v-model="appStore.keyword"
                placeholder="搜索日记内容..."
            />
          </div>
        </div>

        <div class="action-buttons">
          <button
              class="sync-btn"
              @click="syncFromOss()"
              :disabled="isSyncing"
              :title="isSyncing ? '正在同步...' : '从云端同步'"
          >
            <span class="btn-icon" v-if="isSyncing">⏳</span>
            <span class="btn-icon" v-else>☁️</span>
            <span class="btn-text">同步</span>
          </button>
        </div>
      </div>

      <!-- 日记列表 -->
      <section id="list" class="scroll-container" ref="scrollContainer">
        <!-- 列表信息栏 -->
        <div class="list-header" v-if="filteredDiaries.length > 0">
          <div class="list-info">
            <span class="info-text">
              {{ diaryStats.hasSearch ? '搜索到' : '共' }} {{ filteredDiaries.length }} 篇日记
            </span>
            <span class="sort-indicator">按时间排序</span>
          </div>
        </div>

        <!-- 日记卡片列表 -->
        <transition-group name="list" tag="ul" class="diary-list">
          <li
              v-for="diary in filteredDiaries"
              :key="diary.id"
              class="diary-card"
              @click="openDiary(diary)"
          >
            <div class="card-header">
              <div class="date-group">
                <span class="date-primary">
                  <svg viewBox="0 0 24 24" width="14" height="14">
                    <path
                        d="M20 3h-1V1h-2v2H7V1H5v2H4c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 18H4V8h16v13z"/>
                  </svg>
                  {{ formatTimestamp(diary.created) }}
                </span>
                <span
                    class="date-updated"
                    v-if="diary.updated > diary.created"
                    title="最后更新"
                >
                  <span class="update-icon">{{ getCurEmoji(diary.updated) }}</span>
                  {{ formatTimestamp(diary.updated) }}
                </span>
              </div>

              <div class="card-actions">
                <span
                    class="attachment-badge"
                    v-if="diary.attachments?.length"
                    :title="`${diary.attachments.length} 个附件`"
                >
                  <span class="badge-icon">📎</span>
                  <span class="badge-count">{{ diary.attachments.length }}</span>
                </span>
              </div>
            </div>

            <div class="card-content">
              <p class="preview-content">
                {{ diary.content.replace(/<<[A-Z]{3}:[^>]+>>/g, '').trim() || '无内容' }}
              </p>
            </div>

            <div class="card-footer">
              <div class="meta-info">
                <span class="meta-item" v-if="getAttachmentInfo(diary.attachments)">
                  <span class="meta-icon">📦</span>
                  <span class="meta-text">
                    {{ getAttachmentInfo(diary.attachments)!.count }} 个附件
                    <span class="meta-detail">{{ formatBytes(getAttachmentInfo(diary.attachments)?.totalSize) }}</span>
                    <span class="meta-detail" v-if="getAttachmentInfo(diary.attachments)!.imageCount > 0">
                      ({{ getAttachmentInfo(diary.attachments)!.imageCount }} 张图片)
                    </span>
                  </span>
                </span>

                <span class="meta-item diary-id" :title="diary.id">
                  <span class="meta-icon">🆔</span>
                  <span class="meta-text">{{ diary.id.substring(0, 8) }}</span>
                </span>
              </div>

              <span class="open-indicator">
                <svg class="arrow-icon" viewBox="0 0 24 24" width="16" height="16">
                  <path d="M8.59 16.59L13.17 12 8.59 7.41 10 6l6 6-6 6-1.41-1.41z"/>
                </svg>
              </span>
            </div>
          </li>
        </transition-group>

        <!-- 空状态 -->
        <div v-if="filteredDiaries.length === 0" class="empty-state">
          <div class="empty-content">
            <div class="empty-icon">
              <span v-if="isSyncing">⏳</span>
              <span v-else-if="diaryStats.hasSearch">🔍</span>
              <span v-else>📝</span>
            </div>
            <h3 class="empty-title">
              <span v-if="isSyncing">正在同步中...</span>
              <span v-else-if="diaryStats.hasSearch">未找到相关日记</span>
              <span v-else>还没有日记</span>
            </h3>
            <p class="empty-message">
              <span v-if="isSyncing">请稍候，正在从云端同步您的日记...</span>
              <span v-else-if="diaryStats.hasSearch">尝试使用其他关键词搜索</span>
              <span v-else>点击右下角按钮开始写第一篇日记</span>
            </p>
            <button
                v-if="!isSyncing && !diaryStats.hasSearch"
                class="empty-action"
                @click="openDiary(undefined)"
            >
              开始写作
            </button>
          </div>
        </div>
      </section>
    </div>

    <!-- 悬浮新增按钮 -->
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

  .app-header {
    background-color: var(--pad-bg-color-200);
    border-bottom: 1px solid var(--pad-border-color-100);
    padding: 16px 24px 12px;
    flex-shrink: 0;

    .header-content {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 12px;

      .logo-section {
        .app-title {
          font-size: 20px;
          font-weight: 700;
          color: var(--pad-text-color-100);
          margin: 0 0 4px;
          display: flex;
          align-items: center;
          gap: 8px;

          .app-logo {
            width: 32px;
            height: 32px;
            font-size: 24px;
          }
        }
      }

      .stats-section {
        display: flex;
        gap: 16px;

        .stat-item {
          display: flex;
          flex-direction: column;
          align-items: center;
          min-width: 60px;

          &.search-stat {
            .stat-icon {
              background-color: var(--pad-success-color);
            }
          }

          .stat-icon {
            font-size: 20px;
            width: 40px;
            height: 40px;
            display: flex;
            align-items: center;
            justify-content: center;
            background-color: var(--pad-primary-color-light);
            border-radius: var(--pad-radius-full);
            margin-bottom: 4px;
            color: var(--pad-text-color-light);
          }

          .stat-value {
            font-size: 18px;
            font-weight: 700;
            color: var(--pad-text-color-100);
            line-height: 1;
          }

          .stat-label {
            font-size: 11px;
            color: var(--pad-text-color-400);
            margin-top: 2px;
            letter-spacing: 0.3px;
          }
        }
      }
    }
  }

  .main-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0 24px;
  }

  .action-bar {
    display: flex;
    flex-direction: row;
    justify-content: space-between;
    align-items: center;
    padding: 16px 0;
    flex-shrink: 0;

    .search-container {
      flex: 1;
      margin-right: 16px;

      .search-box {
        width: 100%;

        #search-input {
          width: 100%;
          box-sizing: border-box;
          padding: 12px 20px;
          font-size: 15px;
          background-color: var(--pad-bg-color-200);
          border: 1px solid var(--pad-border-color-200);
          border-radius: var(--pad-radius-lg);
          color: var(--pad-text-color-100);
          transition: all var(--pad-transition-fast);

          &:focus {
            outline: none;
            border-color: var(--pad-primary-color);
            box-shadow: 0 0 0 3px var(--pad-primary-color-light);
            background-color: var(--pad-bg-color-100);
          }

          &::placeholder {
            color: var(--pad-text-color-400);
          }
        }
      }
    }

    .action-buttons {
      .sync-btn {
        width: 100px;
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 10px 16px;
        background-color: var(--pad-bg-color-200);
        border: 1px solid var(--pad-border-color-200);
        border-radius: var(--pad-radius-lg);
        color: var(--pad-text-color-200);
        font-size: 14px;
        cursor: pointer;
        transition: all var(--pad-transition-fast);

        &:hover:not(:disabled) {
          background-color: var(--pad-bg-color-300);
          color: var(--pad-text-color-100);
          border-color: var(--pad-border-color-300);
          transform: translateY(-1px);
        }

        &:active:not(:disabled) {
          transform: translateY(0);
        }

        &:disabled {
          opacity: 0.6;
          cursor: not-allowed;
        }

        .btn-icon {
          font-size: 16px;
        }

        .btn-text {
          font-weight: 500;
        }
      }
    }
  }

  .scroll-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding-bottom: 100px;

    // 滚动条样式
    &::-webkit-scrollbar {
      width: 6px;
    }

    &::-webkit-scrollbar-track {
      background: var(--pad-bg-color-200);
      border-radius: var(--pad-radius-full);
    }

    &::-webkit-scrollbar-thumb {
      background: var(--pad-border-color-300);
      border-radius: var(--pad-radius-full);

      &:hover {
        background: var(--pad-border-color-400);
      }
    }

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

        .sort-indicator {
          font-size: 12px;
          color: var(--pad-text-color-400);
          display: flex;
          align-items: center;
          gap: 4px;

          &::before {
            content: '↓';
            font-size: 10px;
          }
        }
      }
    }

    .diary-list {
      list-style: none;
      padding: 0;
      margin: 0;
      position: relative;
    }

    .diary-card {
      background-color: var(--pad-bg-color-200);
      border: 1px solid var(--pad-border-color-100);
      border-radius: var(--pad-radius-lg);
      margin-bottom: 16px;
      padding: 20px;
      cursor: pointer;
      transition: all var(--pad-transition-base);
      box-shadow: var(--pad-shadow-sm);

      &:hover {
        transform: translateY(-2px);
        box-shadow: var(--pad-shadow-md);
        border-color: var(--pad-border-color-300);
        background-color: var(--pad-bg-color-100);
      }

      &:active {
        transform: translateY(0);
      }

      .card-header {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        margin-bottom: 16px;

        .date-group {
          .date-primary {
            display: block;
            font-size: 16px;
            font-weight: 600;
            color: var(--pad-text-color-100);
            margin-bottom: 4px;
          }

          .date-updated {
            display: flex;
            align-items: center;
            gap: 4px;
            font-size: 12px;
            color: var(--pad-text-color-400);

            .update-icon {
              font-size: 10px;
            }
          }
        }

        .card-actions {
          .attachment-badge {
            display: flex;
            align-items: center;
            gap: 4px;
            padding: 4px 8px;
            background-color: var(--pad-bg-color-300);
            border-radius: var(--pad-radius-full);
            font-size: 12px;
            color: var(--pad-text-color-300);
            transition: all var(--pad-transition-fast);

            &:hover {
              background-color: var(--pad-primary-light);
              color: var(--pad-text-color-light);
            }

            .badge-icon {
              font-size: 12px;
            }

            .badge-count {
              font-weight: 600;
            }
          }
        }
      }

      .card-content {
        margin-bottom: 16px;

        .preview-content {
          font-size: 15px;
          line-height: 1.6;
          color: var(--pad-text-color-200);
          margin: 0;
          display: -webkit-box;
          -webkit-box-orient: vertical;
          overflow: hidden;
          text-overflow: ellipsis;
          max-height: 1.6rem;
          white-space: pre-wrap;
        }
      }

      .card-footer {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding-top: 12px;
        border-top: 1px solid var(--pad-border-color-100);

        .meta-info {
          display: flex;
          flex-wrap: wrap;
          gap: 12px;

          .meta-item {
            display: flex;
            align-items: center;
            gap: 4px;
            font-size: 12px;
            color: var(--pad-text-color-400);

            &.diary-id {
              cursor: help;
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .meta-icon {
              font-size: 12px;
              opacity: 0.7;
            }

            .meta-text {
              line-height: 1.3;
            }

            .meta-detail {
              font-size: 11px;
              opacity: 0.8;
            }
          }
        }

        .open-indicator {
          .arrow-icon {
            fill: var(--pad-text-color-400);
            transition: transform var(--pad-transition-fast);
          }
        }
      }

      &:hover .arrow-icon {
        transform: translateX(2px);
      }
    }

    .empty-state {
      display: flex;
      align-items: center;
      justify-content: center;
      min-height: 300px;
      text-align: center;
      padding: 40px 20px;

      .empty-content {
        max-width: 280px;

        .empty-icon {
          font-size: 48px;
          margin-bottom: 20px;
          opacity: 0.7;
        }

        .empty-title {
          font-size: 18px;
          font-weight: 600;
          color: var(--pad-text-color-100);
          margin: 0 0 12px;
        }

        .empty-message {
          font-size: 14px;
          color: var(--pad-text-color-300);
          margin: 0 0 24px;
          line-height: 1.5;
        }

        .empty-action {
          padding: 10px 24px;
          background-color: var(--pad-primary-color);
          color: var(--pad-text-color-light);
          border: none;
          border-radius: var(--pad-radius-lg);
          font-size: 14px;
          font-weight: 500;
          cursor: pointer;
          transition: all var(--pad-transition-fast);

          &:hover {
            background-color: var(--pad-primary-dark);
            transform: translateY(-1px);
          }
        }
      }
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
    .app-header {
      padding: 12px 16px 8px;

      .header-content {
        height: 100%;
        flex-direction: row;
        align-items: flex-start;
        gap: 16px;

        .logo-section {
          width: 100%;
          height: 100%;
          display: flex;
          justify-content: start;
          align-items: center;

          .app-title {
            font-size: 18px;
          }
        }

        .stats-section {
          justify-content: space-between;
          gap: 8px;

          .stat-item {
            min-width: 50px;

            .stat-icon {
              width: 36px;
              height: 36px;
              font-size: 18px;
            }

            .stat-value {
              font-size: 16px;
            }
          }
        }
      }
    }

    .main-content {
      padding: 0 16px;
    }

    .action-bar {
      flex-direction: row;
      align-items: stretch;
      gap: 12px;

      .search-container {
        margin-right: 0;
      }

      .action-buttons {
        align-self: flex-end;
      }
    }

    .diary-card {
      padding: 16px;

      .card-footer {
        .meta-info {
          gap: 8px;

          .meta-item {
            .meta-text {
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
            }
          }
        }
      }
    }

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

    .app-header {
      padding: 16px 20px 12px;
    }
  }
}
</style>
