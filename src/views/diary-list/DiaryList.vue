<script setup lang="ts">
import {computed, onMounted, ref} from "vue";
import {useAppStore} from "../../stores/app.ts";
import {onBeforeRouteLeave, useRouter} from "vue-router";
import DiaryListHeader from "./DiaryListHeader.vue";
import DiarySummaryCard from "../../components/DiarySummaryCard.vue";
import DiaryListEmpty from "./DiaryListEmpty.vue";
import {DiarySummary} from "../../bindings.ts";

const router = useRouter();
const appStore = useAppStore();
const diaryIds = ref<string[]>([]);
const diarySummaries = ref<Record<string, DiarySummary | null>>({});
const isSyncing = ref(false); // 新增同步状态Loading
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

onBeforeRouteLeave((to, _from, next) => {
  // 只有当目标路由是详情页时才保存（可选，但更精确）
  if (scrollContainer.value && to.name === 'DiaryDetail') {
    appStore.savedScrollPosition = scrollContainer.value.scrollTop;
    console.log('保存列表滚动位置:', appStore.savedScrollPosition);
  }
  next();
});

onMounted(async () => {
  console.log("DiaryList mounted");
});
</script>

<template>
  <main id="diary-list">
    <DiaryListHeader :stats="diaryStats"/>

    <div class="main-content">
      <section id="list" class="scroll-container" ref="scrollContainer">
        <transition-group name="list" tag="ul" class="diary-list">
          <DiarySummaryCard
              v-for="id in diaryIds"
              :key="id"
              :diary="diarySummaries[id]"
              @click="openDiary(id)"
          />
        </transition-group>

        <div v-if="diaryIds.length === 0">
          <DiaryListEmpty
              :is-syncing="isSyncing"
              @create="openDiary(undefined)"
          />
        </div>
      </section>
    </div>

    <button
        class="fab"
        @click="openDiary(undefined)"
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
