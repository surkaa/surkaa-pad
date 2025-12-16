<script setup lang="ts">
import {onMounted, onUnmounted, ref, watch, type WatchHandle} from "vue";
import {useAppStore} from "./stores/app.ts";
import {useEventListener} from "./utils/useEventListener.ts";

const appStore = useAppStore();
const watcher = ref<WatchHandle>();

function keydown(event: KeyboardEvent) {
  // 阻止 F5 键
  if (event.key === 'F5') {
    console.log('刷新已被禁用');
    event.preventDefault();
    return;
  }

  // 阻止 Ctrl+R (Windows/Linux) 或 Command+R (Mac)
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'r') {
    console.log('刷新已被禁用');
    event.preventDefault();
    return;
  }
}

function contextmenu(event: MouseEvent) {
  console.log('右键菜单刷新已被禁用');
  event.preventDefault();
  return false;
}

function disableRefresh() {
  useEventListener('keydown', keydown);

  // 阻止右键菜单中的刷新选项
  useEventListener('contextmenu', contextmenu);
}

function syncThemeWithSystem() {
  // 监听系统主题变化
  watcher.value = watch(() => appStore.theme, t => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const applyTheme = (e: MediaQueryListEvent | MediaQueryList) => {
      if (e.matches) {
        document.documentElement.classList.add('dark');
      } else {
        document.documentElement.classList.remove('dark');
      }
    };
    switch (t) {
      case "system":
        applyTheme(mediaQuery);
        mediaQuery.addEventListener('change', applyTheme);
        break;
      case "dark":
        document.documentElement.classList.add('dark');
        mediaQuery.removeEventListener('change', applyTheme);
        break;
      case "light":
        document.documentElement.classList.remove('dark');
        mediaQuery.removeEventListener('change', applyTheme);
        break;
    }
  }, {immediate: true});
}

onMounted(async () => {
  await appStore.initStore();
  disableRefresh();
  syncThemeWithSystem();
});

onUnmounted(() => {
  watcher.value && watcher.value.stop();
});
</script>

<template>
  <router-view/>
</template>
