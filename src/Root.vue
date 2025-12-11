<script setup lang="ts">
import {onMounted} from "vue";

function disableRefresh() {
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
}

onMounted(() => {
  disableRefresh();
});
</script>

<template>
  <router-view/>
</template>
