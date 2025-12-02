<script setup lang="ts">
import {useRoute} from "vue-router";
import {onMounted} from "vue";

const route = useRoute();

const disableRefresh = () => {
  document.addEventListener('keydown', function (event) {
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

  // 阻止右键菜单中的刷新选项
  window.addEventListener('contextmenu', function (event) {
    console.log('右键菜单刷新已被禁用');
    event.preventDefault();
    return false;
  });
};

// TODO 解决移动端可以缩放的问题

onMounted(disableRefresh);
</script>

<template>
  <router-view v-slot="{ Component }">
    <transition name="fade-transform" mode="out-in">
      <component :key="route.path" :is="Component"/>
    </transition>
  </router-view>
</template>

<style scoped lang="scss">
#app-main {
  width: 100%;
  flex: 1;
  overflow: hidden; // 隐藏动画带来的滚动条
  background-color: var(--pad-bg-color-400);

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
    background-color: var(--pad-bg-color-300);
    border-radius: 4px;
  }

  &::-webkit-scrollbar-track {
    background-color: var(--pad-bg-color-100);
  }

  //endregion
}
</style>