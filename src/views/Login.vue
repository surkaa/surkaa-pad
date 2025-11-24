<script setup lang="ts">
import { ref } from 'vue';
import { useAppStore } from '../stores/app';
import { useRouter } from 'vue-router';

const store = useAppStore();
const router = useRouter();

// --- 表单数据 ---
const masterPassword = ref('');
const accessKeyId = ref(store.ossConfig.accessKeyId);
const accessKeySecret = ref(store.ossConfig.accessKeySecret);
const region = ref(store.ossConfig.region);
const endpoint = ref(store.ossConfig.endpoint);
const bucket = ref(store.ossConfig.bucket);

// 1. 首次设置
async function setupAndLogin() {
  if (!masterPassword.value) return;

  const config = {
    accessKeyId: accessKeyId.value,
    accessKeySecret: accessKeySecret.value,
    region: region.value,
    endpoint: endpoint.value,
    bucket: bucket.value
  };

  try {
    await store.handleFirstSetup(masterPassword.value, config);
    router.push({ name: 'Home' });
  } catch (e) {
    // 错误已在 store 中处理
  }
}

// 2. 解锁模式
async function unlock() {
  if (!masterPassword.value) return;
  try {
    await store.handleUnlock(masterPassword.value);
    router.push({ name: 'Home' });
  } catch (e) {
    // 错误已在 store 中处理
  }
}
</script>

<template>
  <div class="login-panel">
    <div v-if="!store.hasSavedConfig">
      <h3>首次配置</h3>
      <input v-model="accessKeyId" placeholder="AccessKey ID" />
      <input v-model="aksecret" type="password" placeholder="AccessKey Secret" />
      <input v-model="region" placeholder="Region (e.g. cn-guangzhou)" />
      <input v-model="endpoint" placeholder="Endpoint" />
      <input v-model="bucket" placeholder="Bucket Name" />
      <input v-model="masterPassword" type="password" placeholder="设置主密码" class="pwd-input" />
      <button @click="setupAndLogin" :disabled="store.isLoadingDerivedKey">
        {{ store.isLoadingDerivedKey ? '处理中...' : '保存配置并登录' }}
      </button>
    </div>

    <div v-else>
      <h3>欢迎回来</h3>
      <input v-model="masterPassword" type="password" placeholder="输入主密码解锁" class="pwd-input" @keyup.enter="unlock" />
      <button @click="unlock" :disabled="store.isLoadingDerivedKey">
        {{ store.isLoadingDerivedKey ? '处理中...' : '解锁' }}
      </button>
      <p class="link-btn" @click="store.resetConfig">重置配置</p>
    </div>

    <p class="status">{{ store.statusMessage }}</p>
  </div>
</template>

<style scoped>
/* 样式与 App.vue 保持一致 */
.login-panel {
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
.pwd-input {
  border: 1px solid #396cd8;
}
button {
  padding: 10px 20px;
  border-radius: 4px;
  border: none;
  cursor: pointer;
  background: #eee;
}
.link-btn {
  color: #396cd8;
  cursor: pointer;
  font-size: 0.9rem;
  text-decoration: underline;
  margin-top: 5px;
}
.status {
  color: #666;
  font-size: 0.9rem;
  margin-top: 10px;
}
</style>