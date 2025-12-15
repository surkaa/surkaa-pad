<script setup lang="ts">
import {useAppStore} from "../../stores/app.ts";

defineProps<{
  keyword: string;
  isSyncing: boolean;
}>();

defineEmits(['update:keyword', 'sync']);

const appStore = useAppStore();

function toggleTheme() {
  switch (appStore.theme) {
    case "system":
      appStore.setTheme("light");
      break;
    case "light":
      appStore.setTheme("dark");
      break;
    case "dark":
      appStore.setTheme("system");
      break;
  }
}
</script>

<template>
  <div class="action-bar">
    <div class="search-container">
      <div class="search-box">
        <input
            id="search-input"
            type="text"
            :value="keyword"
            @input="$emit('update:keyword', ($event.target as HTMLInputElement).value)"
            placeholder="搜索日记"
        />
      </div>
    </div>

    <div class="action-buttons">
      <button
          class="sync-btn"
          @click="$emit('sync')"
          :disabled="isSyncing"
          :title="isSyncing ? '正在同步...' : '从云端同步'"
      >
        <span class="btn-icon rotating" v-if="isSyncing">⏳</span>
        <span class="btn-icon" v-else>☁️</span>
      </button>
      <button
          class="toggle-theme-btn"
          @click="toggleTheme"
      >
        <span class="btn-icon">
          <template v-if="appStore.theme === 'system'">🖥️</template>
          <template v-else-if="appStore.theme === 'light'">🌞</template>
          <template v-else>🌜</template>
        </span>
      </button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.action-bar {
  display: flex;
  flex-direction: row;
  justify-content: space-between;
  align-items: center;
  padding: 4px 18px 8px 18px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--pad-border-color-300);

  .search-container {
    flex: 1;
    margin-right: 16px;

    .search-box {
      width: 100%;

      #search-input {
        width: 100%;
        box-sizing: border-box;
        padding: 12px 16px;
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
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 12px;

    .sync-btn {
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

    .toggle-theme-btn {
      padding: 10px 16px;
      background-color: var(--pad-bg-color-200);
      border: 1px solid var(--pad-border-color-200);
      border-radius: var(--pad-radius-lg);
      color: var(--pad-text-color-200);
      font-size: 14px;
      cursor: pointer;
      transition: all var(--pad-transition-fast);

      &:hover {
        background-color: var(--pad-bg-color-300);
        color: var(--pad-text-color-100);
        border-color: var(--pad-border-color-300);
        transform: translateY(-1px);
      }

      &:active {
        transform: translateY(0);
      }

      .btn-text {
        font-weight: 500;
      }
    }
  }
}

@media (max-width: 512px) {
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
}
</style>
