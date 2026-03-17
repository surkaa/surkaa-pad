<script setup lang="ts">
import {onMounted, onUnmounted} from "vue";
import {platform} from "@tauri-apps/plugin-os";
import {useEventListener} from "@vueuse/core";
import {useConfigStore} from "./stores/config.ts";
import {UnlistenFn} from "@tauri-apps/api/event";

const configStore = useConfigStore();
const p = platform();
let unListener: UnlistenFn | null = null;

if (p === 'windows') {
  useEventListener('keydown', (event: KeyboardEvent) => {
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
  });

  useEventListener('contextmenu', (e) => {
    e.preventDefault();
  });
}

onMounted(async () => {
  unListener = await configStore.watchConfig('app-theme', (t) => {
    console.log('theme change', t);
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const applyTheme = (e: MediaQueryListEvent | MediaQueryList) => {
      if (e.matches) {
        document.body.classList.add('body--dark');
      } else {
        document.body.classList.remove('body--dark');
      }
    };
    switch (t) {
      case "dark":
        document.body.classList.add('body--dark');
        mediaQuery.removeEventListener('change', applyTheme);
        break;
      case "light":
        document.body.classList.remove('body--dark');
        mediaQuery.removeEventListener('change', applyTheme);
        break;
      case undefined:
      case "system":
        applyTheme(mediaQuery);
        mediaQuery.addEventListener('change', applyTheme);
        break;
    }
  }, true);
});

onUnmounted(() => {
  if (unListener) {
    unListener();
    unListener = null;
  }
})
</script>

<template>
  <router-view v-slot="{ Component }">
    <component :is="Component"/>
  </router-view>
</template>
