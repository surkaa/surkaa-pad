<script setup lang="ts">
import {onMounted} from "vue";
import {useAppStore} from './stores/app';
import {useRouter} from 'vue-router';

// 导入视图组件

const store = useAppStore();
const router = useRouter();

onMounted(async () => {
  // 1. 初始化 Pinia Store 中的本地存储
  await store.initializeStore();

  // 2. 根据初始状态进行导航
  if (!store.hasSavedConfig) {
    router.push({ name: 'Login' });
  } else if (!store.isLoggedIn) {
    router.push({ name: 'Login' });
  } else {
    router.push({ name: 'Home' });
  }
});

function navigateToLogin() {
  router.push({ name: 'Login' });
}
</script>

<template>
  <main class="container">
    <header class="header">
      <h2>Surkaa Pad</h2>
      <button
          v-if="store.isLoggedIn && store.viewMode === 'editor'"
          @click="store.viewMode = 'list'"
          class="small-btn">
        返回列表
      </button>
      <button
          v-else-if="!store.isLoggedIn"
          @click="navigateToLogin"
          class="small-btn">
        设置/解锁
      </button>
    </header>

    <router-view />
  </main>
</template>

<style>
.container {
  padding: 20px;
  max-width: 600px;
  margin: 0 auto;
  font-family: sans-serif;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
  border-bottom: 1px solid #eee;
  padding-bottom: 10px;
}

.login-panel, .app-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

input, textarea {
  padding: 10px;
  border: 1px solid #ddd;
  border-radius: 4px;
  width: 100%;
  box-sizing: border-box;
  margin-bottom: 10px;
}

button {
  padding: 10px 20px;
  border-radius: 4px;
  border: none;
  cursor: pointer;
  background: #eee;
}

.primary-btn {
  background: #396cd8;
  color: white;
  width: 100%;
}

.small-btn {
  padding: 5px 10px;
  font-size: 0.8rem;
}

.status {
  color: #666;
  font-size: 0.9rem;
  margin-top: 10px;
}

.link-btn {
  color: #396cd8;
  cursor: pointer;
  font-size: 0.9rem;
  text-decoration: underline;
  margin-top: 5px;
}

.diary-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 20px;
}

.diary-item {
  padding: 15px;
  background: #fff;
  border: 1px solid #eee;
  border-radius: 8px;
  cursor: pointer;
  display: flex;
  justify-content: space-between;
}

.diary-item:hover {
  background: #f9f9f9;
}

.date {
  font-weight: bold;
}

.id-preview {
  color: #999;
  font-size: 0.8rem;
}

.search-results {
  margin-top: 10px;
  border: 1px solid #eee;
  padding: 10px;
  border-radius: 4px;
  background: #fafafa;
}

.result-card {
  border-bottom: 1px solid #ddd;
  padding: 10px 0;
}
</style>