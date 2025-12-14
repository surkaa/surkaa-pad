<script setup lang="ts">
defineProps<{
  stats: {
    total: number;
    filtered: number;
    withAttachments: number;
    lastUpdated: number;
    hasSearch: boolean;
    searchCount: number;
  }
}>();
</script>

<template>
  <header class="app-header">
    <div class="header-content">
      <div class="logo-section">
        <h1 class="app-title">
          <img alt="app-logo" class="app-logo" src="/app-icon.png"/>
          SurKaa Pad
        </h1>
      </div>

      <div class="stats-section" v-if="!stats.hasSearch">
        <div class="stat-item">
          <span class="stat-icon">📚</span>
          <span class="stat-value">{{ stats.total }}</span>
          <span class="stat-label">篇日记</span>
        </div>
        <div class="stat-item" v-if="stats.withAttachments > 0">
          <span class="stat-icon">📎</span>
          <span class="stat-value">{{ stats.withAttachments }}</span>
          <span class="stat-label">含附件</span>
        </div>
      </div>
      <div class="stats-section" v-else>
        <div class="stat-item search-stat">
          <span class="stat-icon">🔍</span>
          <span class="stat-value">{{ stats.searchCount }}</span>
          <span class="stat-label">条结果</span>
        </div>
      </div>
    </div>
  </header>
</template>

<style scoped lang="scss">
.app-header {
  background-color: var(--pad-bg-color-100);
  padding: 16px 24px 12px;
  flex-shrink: 0;

  .header-content {
    display: flex;
    justify-content: space-between;
    align-items: center;

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

@media (max-width: 512px) {
  .app-header {
    padding: 4px 16px 8px;

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
}

@media (min-width: 513px) and (max-width: 768px) {
  .app-header {
    padding: 16px 20px 12px;
  }
}
</style>
