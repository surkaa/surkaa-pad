<script setup lang="ts">
import {ref} from "vue";
import { invoke } from "@tauri-apps/api/core";

// --- State Variables ---
const akid = ref('');
const aksecret = ref('');
const region = ref('cn-guangzhou');
const endpoint = ref('oss-cn-guangzhou.aliyuncs.com'); // 默认值
const bucket = ref('surkaa'); // 默认值
const masterPassword = ref('');

// --- Global State (In-Memory for simplicity) ---
// 存储派生密钥
const dek = ref<number[]>([]);
const saltBase64 = "aHR0cHM6Ly9nZW1pbmkuZ29vZ2xlLmNvbS9hcHAvMDU5MmNjODMwNzQ4MWQ0OA==".replace(/=/g, '');
const statusMessage = ref('等待配置...');

// --- Functions ---

/**
 * 完整登录和初始化流程
 */
async function handleLogin() {
  statusMessage.value = '正在登录和初始化...';

  // 1. 派生密钥 (KDF)
  try {
    const derivedKey = await invoke<number[]>('derive_key', {
      password: masterPassword.value,
      salt: saltBase64
    });
    dek.value = derivedKey;
    console.log('DEK 派生成功:', derivedKey.length === 32);
    statusMessage.value = '密钥派生成功，正在初始化 OSS 客户端...';
  } catch (error) {
    statusMessage.value = `密钥派生失败: ${error}`;
    console.error(error);
    return;
  }

  // 2. 初始化 OSS 客户端
  try {
    await invoke('initialize_oss_client', {
      akId: akid.value,
      akSecret: aksecret.value,
      region: region.value,
      endpoint: endpoint.value,
      bucket: bucket.value,
    });
    statusMessage.value = 'OSS 客户端初始化成功！应用已准备就绪。';
    console.log('OSS 客户端初始化成功');
  } catch (error) {
    statusMessage.value = `OSS 初始化失败: ${error}`;
    console.error(error);
  }
}
</script>

<template>
  <main class="container">
  <h1>Surkaa Pad - 初始化</h1>

  <div class="row">
    <label for="password-input">主密码:</label>
    <input
        id="password-input"
        v-model="masterPassword"
        placeholder="输入主密码"
        type="password"
    />
  </div>

  <div class="row">
    <label for="akid-input">AccessKey ID:</label>
    <input
        id="akid-input"
        v-model="akid"
        placeholder="阿里云 AccessKey ID"
    />
  </div>

  <div class="row">
    <label for="aksecret-input">AccessKey Secret:</label>
    <input
        id="aksecret-input"
        v-model="aksecret"
        placeholder="阿里云 AccessKey Secret"
        type="password"
    />
  </div>

  <div class="row">
    <label for="region-input">地域 (Region):</label>
    <input
        id="region-input"
        v-model="region"
        placeholder="cn-guangzhou"
    />
  </div>

  <div class="row">
    <label for="endpoint-input">Endpoint:</label>
    <input
        id="endpoint-input"
        v-model="endpoint"
        placeholder="oss-cn-guangzhou.aliyuncs.com"
    />
  </div>

  <div class="row">
    <label for="bucket-input">Bucket:</label>
    <input
        id="bucket-input"
        v-model="bucket"
        placeholder="您的 Bucket 名称"
    />
  </div>

  <button @click="handleLogin" :disabled="!masterPassword">
    保存配置并登录
  </button>

  <p style="margin-top: 15px;">状态: {{ statusMessage }}</p>
  <p v-if="dek.length > 0">DEK就绪 ({{ dek.length }} bytes)</p>
</main>
</template>

<style scoped>
/* 添加一些简单的布局样式 */
.row {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  margin-bottom: 15px;
  width: 300px; /* 限制宽度 */
  margin-left: auto;
  margin-right: auto;
}
.row label {
  margin-bottom: 5px;
  font-weight: bold;
}
.row input {
  width: 100%;
}
</style>
<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0;
  padding-top: 10vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  text-align: center;
}

a {
  font-weight: 500;
  color: #646cff;
  text-decoration: inherit;
}

a:hover {
  color: #535bf2;
}

h1 {
  text-align: center;
}

input,
button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
}

button {
  cursor: pointer;
}

button:hover {
  border-color: #396cd8;
}

button:active {
  border-color: #396cd8;
  background-color: #e8e8e8;
}

input,
button {
  outline: none;
}

#greet-input {
  margin-right: 5px;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  a:hover {
    color: #24c8db;
  }

  input,
  button {
    color: #ffffff;
    background-color: #0f0f0f98;
  }

  button:active {
    background-color: #0f0f0f69;
  }
}

</style>