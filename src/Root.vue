<script setup lang="ts">
import {onMounted, watch} from "vue";
import {useAppStore} from "./stores/app.ts";
import {useEventListener} from "./utils/useEventListener.ts";
import {platform} from "@tauri-apps/plugin-os";

const appStore = useAppStore();
const p = platform();

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

watch(() => appStore.theme, t => {
  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
  const applyTheme = (e: MediaQueryListEvent | MediaQueryList) => {
    if (e.matches) {
      document.body.classList.add('body--dark');
    } else {
      document.body.classList.remove('body--dark');
    }
  };
  switch (t) {
    case "system":
      applyTheme(mediaQuery);
      mediaQuery.addEventListener('change', applyTheme);
      break;
    case "dark":
      document.body.classList.add('body--dark');
      mediaQuery.removeEventListener('change', applyTheme);
      break;
    case "light":
      document.body.classList.remove('body--dark');
      mediaQuery.removeEventListener('change', applyTheme);
      break;
  }
}, {immediate: true});

onMounted(appStore.initStore);
</script>

<template>
  <router-view v-slot="{ Component }">
    <component :is="Component"/>
  </router-view>
</template>
