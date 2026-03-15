<script setup lang="ts">
import {useRoute, useRouter} from "vue-router";
import {computed} from "vue";
import {useLayoutStore} from "../stores/layout.ts";
import {keepAliveIncludes} from "../composables/useKeepAlive.ts";

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
  <q-layout view="hHh lpR fFf" class="app-layout">
    <q-header bordered class="app-header">
      <div class="header-left q-px-md row items-center full-height">
        <q-btn flat round icon="arrow_back" v-if="canBack" @click="back" size="sm"/>
        <div class="header-global q-ml-sm" v-if="!route.meta.searchMod">
          {{ layoutStore.customTitle || route.meta.title || 'SurKaa Pad' }}
        </div>
        <q-space v-if="!route.meta.searchMod"/>
        <div id="header-actions"></div>
      </div>
    </q-header>

    <q-page-container>
      <div class="app-content" :style="{ height: `calc(100vh - ${route.meta.hideFooter ? 60 : 95}px)` }">
        <router-view v-slot="{ Component }">
          <keep-alive :include="keepAliveIncludes">
            <component :is="Component" :key="route.fullPath"/>
          </keep-alive>
        </router-view>
      </div>
    </q-page-container>

    <q-footer bordered v-if="!route.meta.hideFooter" class="app-footer">
      <div id="footer-content" class="full-width q-px-md row justify-between items-center">
      </div>
    </q-footer>
  </q-layout>
</template>

<style scoped lang="scss">
.app-layout {
  background-color: var(--pad-bg-color);
  color: var(--pad-text-color);

  .app-header {
    height: 60px;
  }

  .app-content {
    overflow: hidden;
  }

  .app-footer {
    height: 35px;
    display: flex;
    align-items: center;
  }
}
</style>
