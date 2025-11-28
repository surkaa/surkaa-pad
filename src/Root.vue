<script setup lang="ts">
import {useRoute} from "vue-router";

const route = useRoute();
</script>

<template>
  <router-view v-slot="{ Component }">
    <transition name="fade-transform" mode="out-in">
      <keep-alive>
        <component :key="route.path" :is="Component"/>
      </keep-alive>
    </transition>
  </router-view>
</template>

<style scoped lang="scss">
#app-main {
  width: 100%;
  flex: 1;
  overflow: hidden; // 隐藏动画带来的滚动条
  background-color: var(--template-bg-color-400);

  .fade-transform-leave-active,
  .fade-transform-enter-active {
    transition: all .3s;
  }

  .fade-transform-enter {
    opacity: 0;
    transform: translateX(-30px);
  }

  .fade-transform-leave-to {
    opacity: 0;
    transform: translateX(30px);
  }

  //region 滚动条样式
  &::-webkit-scrollbar {
    width: 8px;
    height: 8px;
  }

  &::-webkit-scrollbar-thumb {
    background-color: var(--template-bg-color-300);
    border-radius: 4px;
  }

  &::-webkit-scrollbar-track {
    background-color: var(--template-bg-color-100);
  }

  //endregion
}
</style>