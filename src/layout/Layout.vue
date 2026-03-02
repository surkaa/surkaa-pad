<script setup lang="ts">
import {useRoute, useRouter} from "vue-router";
import {computed} from "vue";
import {useLayoutStore} from "../stores/layout.ts";
import { keepAliveIncludes } from "../composables/useKeepAlive.ts";

const route = useRoute();
const router = useRouter();
const layoutStore = useLayoutStore();

const canBack = computed(() => {
  route.fullPath;
  return window.history.state.back !== null;
});

function back() {
  router.back();
}
</script>

<template>
  <div class="app-layout">
    <header class="app-header">
      <div class="header-left">
        <div class="header-back" v-if="canBack" @click="back"><</div>
        <div class="header-global">{{ layoutStore.customTitle || route.meta.title || 'SurKaa Pad' }}</div>
      </div>
      <div id="header-actions"></div>
    </header>

    <main class="app-content">
      <router-view v-slot="{ Component }">
        <keep-alive :include="keepAliveIncludes">
          <component :is="Component" :key="route.fullPath" />
        </keep-alive>
      </router-view>
    </main>

    <footer class="app-footer">
      <div id="footer-content">
      </div>
    </footer>
  </div>
</template>

<style scoped lang="scss">
.app-layout {
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
  color: var(--pad-text-color);

  .app-header {
    height: 60px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 20px;
    border-bottom: 1px solid var(--pad-border-color-200);

    .header-left {
      display: flex;
      align-items: center;

      .header-back {
        cursor: pointer;
        margin-right: 16px;
        font-size: 20px;
        user-select: none;
      }

      .header-global {
        font-weight: bold;
        font-size: 18px;
      }
    }
  }

  .app-content {
    flex: 1;
    max-height: calc(100% - 60px - 35px);
  }

  .app-footer {
    height: 35px;
    border-top: 1px solid var(--pad-border-color-200);
    display: flex;
    align-items: center;
    justify-content: center;

    #footer-content {
      width: 100%;
      padding: 0 1rem;
      display: flex;
      justify-content: space-between;
    }
  }
}
</style>
