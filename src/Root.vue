<script setup lang="ts">
import {Channel} from '@tauri-apps/api/core';
import {relaunch} from '@tauri-apps/plugin-process';
import {platform} from "@tauri-apps/plugin-os";
import {useEventListener} from "@vueuse/core";
import {useConfigStore} from "./stores/config.ts";
import {onMounted, ref, watchEffect} from "vue";
import type {LocalStorageMigrationEvent} from './bindings';
import LocalStorageMigrationDialog from './components/LocalStorageMigrationDialog.vue';
import api from './utils/api';
import {formatError} from './utils/formatError';
import {
  initialLocalStorageMigrationDisplay,
  reduceLocalStorageMigrationDisplay,
  withLocalStorageMigrationError,
} from './utils/localStorageMigration';

const configStore = useConfigStore();
const p = platform();
const showLocalStorageMigration = ref(false);
const localStorageMigrationDisplay = ref(initialLocalStorageMigrationDisplay());

if (p === 'windows') {
  useEventListener('keydown', (event: KeyboardEvent) => {
    // 阻止 F5 键
    if (event.key === 'F5') {
      console.log('刷新已被禁用');
      event.preventDefault();
      return;
    }

    // 阻止普通的 Ctrl+R 刷新，带 Alt/Shift 的组合保留给编辑器快捷键
    if (
      (event.ctrlKey || event.metaKey)
      && !event.altKey
      && !event.shiftKey
      && event.key.toLowerCase() === 'r'
    ) {
      console.log('刷新已被禁用');
      event.preventDefault();
      return;
    }
  });

  useEventListener('contextmenu', (e) => {
    e.preventDefault();
  });
}

const theme = configStore.useTauriConfig('app-theme');
const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

// DOM 副作用交给 watchEffect 托管，theme 变化时自动重新执行
watchEffect((onCleanup) => {
  const applySystemTheme = (e: MediaQueryListEvent | MediaQueryList) => {
    document.body.classList.toggle('body--dark', e.matches);
  };

  switch (theme.value) {
    case "dark":
      document.body.classList.add('body--dark');
      break;
    case "light":
      document.body.classList.remove('body--dark');
      break;
    default:
      // 只有 system 或 undefined 时挂载系统级监听
      applySystemTheme(mediaQuery);
      mediaQuery.addEventListener('change', applySystemTheme);

      // onCleanup 用于在这个 watchEffect 重新执行前清理上一次的事件监听器
      onCleanup(() => {
        mediaQuery.removeEventListener('change', applySystemTheme);
      });
      break;
  }
});

onMounted(checkStartupLocalStorageMigration);

async function checkStartupLocalStorageMigration() {
  try {
    const status = await api.cmdGetLocalStorageMigrationStatus();
    if (status.legacyMigrationRequired || status.migrationPending) {
      await migrateLegacyLocalStorage();
    }
  } catch (error) {
    console.error('检查本地数据迁移状态失败:', error);
  }
}

async function migrateLegacyLocalStorage() {
  showLocalStorageMigration.value = true;
  localStorageMigrationDisplay.value = initialLocalStorageMigrationDisplay();
  const event = new Channel<LocalStorageMigrationEvent>();
  event.onmessage = (message) => {
    localStorageMigrationDisplay.value = reduceLocalStorageMigrationDisplay(
      localStorageMigrationDisplay.value,
      message,
    );
  };

  try {
    await api.cmdMigrateLocalStorage(event, null);
  } catch (error) {
    if (!localStorageMigrationDisplay.value.error) {
      localStorageMigrationDisplay.value = withLocalStorageMigrationError(
        localStorageMigrationDisplay.value,
        formatError(error),
      );
    }
  }
}
</script>

<template>
  <router-view v-slot="{ Component }">
    <component :is="Component"/>
  </router-view>
  <LocalStorageMigrationDialog
    v-model="showLocalStorageMigration"
    title="升级本地数据目录"
    :display="localStorageMigrationDisplay"
    allow-defer
    @retry="migrateLegacyLocalStorage"
    @defer="showLocalStorageMigration = false"
    @restart="relaunch"
  />
</template>
