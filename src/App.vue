<script setup lang="ts">
import {onMounted, ref} from "vue";
import {invoke} from "@tauri-apps/api/core";
import {Store} from "@tauri-apps/plugin-store";
import {downloadFile, initOSS, uploadFile} from "./utils/alioss.ts";
import {DiaryEntry, SearchResult} from "./types";

// --- 常量 ---
const CONFIG_FILENAME = "settings.json";
const CONFIG_KEY = "encrypted_oss_config";

// --- 存储实例 (非响应式，初始化为 null) ---
let store: Store | null = null;
let isLoadingDerivedKey = false;

// --- 状态变量 ---
const hasSavedConfig = ref(false); // 是否存在本地配置
const isLoggedIn = ref(false);     // 是否已登录
const viewMode = ref<'list' | 'editor'>('list'); // 当前视图模式

// --- 表单数据 ---
const masterPassword = ref('');
const akid = ref('');
const aksecret = ref('');
const region = ref('cn-guangzhou');
const endpoint = ref('oss-cn-guangzhou.aliyuncs.com');
const bucket = ref('surkaa');

// --- 全局状态 ---
const dek = ref<number[]>([]);
const saltBase64 = "aHR0cHM6Ly9nZW1pbmkuZ29vZ2xlLmNvbS9hcHAvMDU5MmNjODMwNzQ4MWQ0OA==".replace(/=/g, '');
const statusMessage = ref('初始化中...');

// --- 日记数据 ---
const diaryList = ref<DiaryEntry[]>([]); // 日记列表
const currentEntryId = ref<number | null>(null);   // 当前编辑的 ID (空代表新建)
const currentDiaryContent = ref('');
const keywordsInput = ref('');
const searchKeyword = ref('');
const searchResults = ref<SearchResult[]>([]);

// ==========================================
// 生命周期与初始化
// ==========================================

onMounted(async () => {
  try {
    // 1. 在 onMounted 中加载 Store，避免顶层 await 导致白屏
    store = await Store.load(CONFIG_FILENAME);
    // 2. 检查配置
    await checkSavedConfig();
  } catch (e) {
    console.error("Store 加载失败:", e);
    statusMessage.value = "本地存储初始化失败。";
  }
});

async function checkSavedConfig() {
  if (!store) return;
  try {
    const val = await store.get(CONFIG_KEY);
    if (val) {
      hasSavedConfig.value = true;
      statusMessage.value = "发现本地配置，请输入密码解锁。";
    } else {
      statusMessage.value = "无本地配置，请进行首次设置。";
    }
  } catch (e) {
    console.error("读取配置失败:", e);
  }
}

// ==========================================
// 登录逻辑 (首次设置 & 解锁)
// ==========================================

// 1. 首次设置
async function handleFirstSetup() {
  if (!masterPassword.value || !store) return;
  statusMessage.value = "正在验证配置...";

  try {
    if (isLoadingDerivedKey) {
      statusMessage.value = "别急，正在验证配置...";
      return;
    }
    isLoadingDerivedKey = true;
    const derivedKey = await invoke<number[]>('derive_key', {
      password: masterPassword.value,
      salt: saltBase64
    });

    // --- 修改点：前端初始化 OSS ---
    // ali-oss 的 region 格式通常是 'oss-cn-guangzhou'
    await initOSS({
      accessKeyId: akid.value,
      accessKeySecret: aksecret.value,
      region: region.value,
      endpoint: endpoint.value,
      bucket: bucket.value,
    });
    // ---------------------------

    const configObj = {
      akid: akid.value,
      aksecret: aksecret.value,
      region: region.value,
      endpoint: endpoint.value,
      bucket: bucket.value
    };
    const configJson = JSON.stringify(configObj);

    const encryptedConfig = await invoke<number[]>('encrypt_config', {
      dek: derivedKey,
      configJson: configJson
    });

    await store.set(CONFIG_KEY, encryptedConfig);
    await store.save();

    dek.value = derivedKey;
    isLoggedIn.value = true;
    hasSavedConfig.value = true;
    statusMessage.value = "登录成功。";

    await loadDiaryList();

  } catch (e) {
    statusMessage.value = `设置失败: ${e}`;
    console.error(e);
  } finally {
    isLoadingDerivedKey = false;
  }
}

// 2. 解锁模式
async function handleUnlock() {
  if (!masterPassword.value || !store) return;
  statusMessage.value = "正在解锁...";

  try {
    if (isLoadingDerivedKey) {
      statusMessage.value = "别急";
      return;
    }
    isLoadingDerivedKey = true;
    const derivedKey = await invoke<number[]>('derive_key', {
      password: masterPassword.value,
      salt: saltBase64
    });

    const encryptedConfig = await store.get<number[]>(CONFIG_KEY);
    if (!encryptedConfig) throw "配置文件丢失";

    const configJson = await invoke<string>('decrypt_config', {
      dek: derivedKey,
      encryptedData: encryptedConfig
    });

    const config = JSON.parse(configJson);

    akid.value = config.akid;
    aksecret.value = config.aksecret;
    region.value = config.region;
    endpoint.value = config.endpoint;
    bucket.value = config.bucket;

    // --- 修改点：前端初始化 OSS ---
    await initOSS({
      accessKeyId: config.akid,
      accessKeySecret: config.aksecret,
      region: config.region,
      endpoint: config.endpoint,
      bucket: config.bucket,
    });
    // ---------------------------

    dek.value = derivedKey;
    isLoggedIn.value = true;
    statusMessage.value = "解锁成功。";

    await loadDiaryList();

  } catch (e) {
    statusMessage.value = `解锁失败: ${e}`;
    console.error(e);
  } finally {
    isLoadingDerivedKey = false;
  }
}

async function resetConfig() {
  if (!store) return;
  await store.delete(CONFIG_KEY);
  await store.save();
  hasSavedConfig.value = false;
  statusMessage.value = "配置已重置。";
}

// ==========================================
// 业务逻辑
// ==========================================

async function loadDiaryList() {
  try {
    diaryList.value = await invoke<any[]>('get_all_entries');
  } catch (e) {
    console.error("加载列表失败", e);
  }
}

function openNewEntry() {
  currentEntryId.value = null;
  currentDiaryContent.value = '';
  keywordsInput.value = '';
  viewMode.value = 'editor';
  statusMessage.value = '新建日记模式';
}

async function handleSaveDiary() {
  if (!dek.value.length || !currentDiaryContent.value) return;
  statusMessage.value = '正在加密...';

  const id = currentEntryId.value || Date.now();
  const createdAt = Date.now();
  const keywords = keywordsInput.value.split(',').map(k => k.trim()).filter(k => k.length > 0);

  try {
    const [ciphertext, iv] = await invoke<[number[], number[]]>('encrypt_data', {
      dek: dek.value,
      plaintext: currentDiaryContent.value,
    });
    const fullEncryptedData = [...iv, ...ciphertext];

    for (const keyword of keywords) {
      const search_hash = await invoke<number[]>('generate_search_hash', {
        dek: dek.value,
        keyword,
      });
      await invoke('save_local_index', {
        id,
        nonce: iv,
        createdAt: createdAt,
        searchHash: search_hash,
      });
    }

    // --- 修改点：使用前端上传 ---
    statusMessage.value = '正在上传到 OSS...';
    const objectKey = `data/${id}.dat`;

    // fullEncryptedData 是 Rust 返回的 number[]
    await uploadFile(objectKey, fullEncryptedData);
    // ---------------------------

    statusMessage.value = "保存成功！";
    currentEntryId.value = null;
    currentDiaryContent.value = '';
    keywordsInput.value = '';

    await loadDiaryList();
    viewMode.value = 'list';

  } catch (e) {
    statusMessage.value = `保存失败: ${e}`;
    console.error(e);
  }
}

// 补全的 Search 函数
async function handleSearch() {
  if (dek.value.length !== 32 || !searchKeyword.value) return;
  statusMessage.value = '正在搜索...';

  try {
    const search_hash = await invoke<number[]>('generate_search_hash', {
      dek: dek.value,
      keyword: searchKeyword.value.trim(),
    });

    const matchedEntries = await invoke<any[]>('search_local_index', {
      searchHash: search_hash,
    });

    if (matchedEntries.length === 0) {
      statusMessage.value = `未找到匹配项。`;
      searchResults.value = [];
      return;
    }

    statusMessage.value = `找到 ${matchedEntries.length} 条，正在下载解密...`;
    const decryptedResults = [] as SearchResult[];

    for (const entry of matchedEntries) {
      const objectKey = `data/${entry.id}.dat`;

      // 下载
      const fullEncryptedData = await downloadFile(objectKey);

      // 解密
      const ivLength = 12;
      const ciphertextWithTag = fullEncryptedData.slice(ivLength);

      const plaintext = await invoke<string>('decrypt_data', {
        dek: dek.value,
        ciphertext: ciphertextWithTag,
        nonceBytes: entry.nonce,
      });

      decryptedResults.push({
        id: entry.id,
        created_at: entry.created_at,
        nonce: entry.nonce,
        content: plaintext,
      });
    }

    searchResults.value = decryptedResults;
    statusMessage.value = `搜索完成。`;

  } catch (error) {
    statusMessage.value = `搜索失败: ${error}`;
    searchResults.value = [];
  }
}

async function handleEntryClick(entry: any) {
  if (!dek.value.length) return;

  statusMessage.value = `正在下载 ID: ${entry.id}...`;
  viewMode.value = 'editor';

  currentEntryId.value = entry.id;
  currentDiaryContent.value = '加载中...';
  keywordsInput.value = '';

  try {
    const objectKey = `data/${entry.id}.dat`;

    // --- 修改点：使用前端下载 ---
    const fullEncryptedData = await downloadFile(objectKey);
    // ---------------------------

    statusMessage.value = "正在解密...";

    // 注意：Rust 期望接收 number[] 或 Vec<u8>，我们前端工具类已经确保返回 number[]
    const ivLength = 12;
    const ciphertextWithTag = fullEncryptedData.slice(ivLength);

    currentDiaryContent.value = await invoke<string>('decrypt_data', {
      dek: dek.value,
      ciphertext: ciphertextWithTag,
      nonceBytes: entry.nonce, // 使用列表中的 Nonce
    });
    statusMessage.value = `加载成功`;

  } catch (e) {
    statusMessage.value = `加载失败: ${e}`;
    currentDiaryContent.value = '';
    console.error(e);
  }
}

</script>

<template>
  <main class="container">
    <header class="header">
      <h2>Surkaa Pad</h2>
      <button v-if="isLoggedIn && viewMode === 'editor'" @click="viewMode = 'list'" class="small-btn">返回列表</button>
    </header>

    <div v-if="!isLoggedIn" class="login-panel">
      <div v-if="!hasSavedConfig">
        <h3>首次配置</h3>
        <input v-model="akid" placeholder="AccessKey ID"/>
        <input v-model="aksecret" type="password" placeholder="AccessKey Secret"/>
        <input v-model="region" placeholder="Region (e.g. cn-guangzhou)"/>
        <input v-model="endpoint" placeholder="Endpoint"/>
        <input v-model="bucket" placeholder="Bucket Name"/>
        <input v-model="masterPassword" type="password" placeholder="设置主密码" class="pwd-input"/>
        <button @click="handleFirstSetup">保存配置并登录</button>
      </div>

      <div v-else>
        <h3>欢迎回来</h3>
        <input v-model="masterPassword" type="password" placeholder="输入主密码解锁" class="pwd-input"/>
        <button @click="handleUnlock">解锁</button>
        <p class="link-btn" @click="resetConfig">重置配置</p>
      </div>

      <p class="status">{{ statusMessage }}</p>
    </div>

    <div v-else class="app-panel">

      <div v-if="viewMode === 'list'" class="list-view">
        <div class="list-actions">
          <button @click="openNewEntry" class="primary-btn">+ 新建日记</button>
        </div>

        <div class="diary-list">
          <div v-for="item in diaryList" :key="item.id" class="diary-item" @click="handleEntryClick(item)">
            <span class="date">{{ new Date(item.created_at).toLocaleString() }}</span>
            <span class="id-preview">ID: {{ item.id }}</span>
          </div>
          <p v-if="diaryList.length === 0" style="color:#999">暂无本地记录</p>
        </div>
      </div>

      <div v-else class="editor-view">
        <div class="search-box">
          <input v-model="searchKeyword" placeholder="输入关键词搜索..."/>
          <button @click="handleSearch">搜索本地索引</button>
        </div>

        <div v-if="searchResults.length > 0" class="search-results">
          <div v-for="result in searchResults" :key="result.id" class="result-card">
            <small>{{ result.created_at }}</small>
            <p style="white-space: pre-wrap;">{{ result.content }}</p>
          </div>
        </div>

        <hr/>

        <h3>{{ currentEntryId ? '编辑/查看' : '新建日记' }}</h3>
        <input v-model="keywordsInput" placeholder="关键词 (逗号分隔)"/>
        <textarea v-model="currentDiaryContent" rows="12" placeholder="写点什么..."></textarea>
        <button @click="handleSaveDiary" class="primary-btn">加密上传</button>

        <p class="status">{{ statusMessage }}</p>
      </div>
    </div>
  </main>
</template>

<style scoped>
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