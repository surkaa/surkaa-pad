<script setup lang="ts">
import {platform} from "@tauri-apps/plugin-os";
import {useEventListener} from "@vueuse/core";
import {useConfigStore} from "./stores/config.ts";
import {watchEffect} from "vue";

const configStore = useConfigStore();
const p = platform();

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
</script>

<template>
  <router-view v-slot="{ Component }">
    <component :is="Component"/>
  </router-view>
</template>
