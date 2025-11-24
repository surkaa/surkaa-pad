<script setup lang="ts">
import {ref, watch, computed} from 'vue';
import {useAppStore} from '../stores/app';
import {invoke} from "@tauri-apps/api/core";
import {downloadFile, downloadFileHead, uploadDiaryFile} from "../utils/alioss";
import {BatchIndexEntry, EncryptData, KeywordToken, PageSearchResult, SearchIndexResult, DiaryEntry} from "../types";

const store = useAppStore();

// --- 编辑器/搜索状态 ---
const currentDiaryContent = ref('');
const searchKeyword = ref('');
const searchResults = ref<PageSearchResult[]>([]);

// 计算属性：当前 ID 是否已设置（用于区分新建/编辑）
const isNewEntry = computed(() => store.currentEntryId === null);

// 监听 viewMode 变化，在进入 editor 时清空搜索结果和内容
watch(() => store.viewMode, (newVal) => {
  if (newVal === 'editor' && isNewEntry.value) {
    currentDiaryContent.value = ''; // 新建时清空内容
  }
  searchResults.value = []; // 每次切换或进入编辑器时清空搜索结果
});


// ==========================================
// CRUD (增删改查) & 索引
// ==========================================

async function handleSaveDiary() {
  if (!store.dek.length || !currentDiaryContent.value) return;
  store.statusMessage = '正在加密...';

  const id = store.currentEntryId || Date.now();

  try {
    // 1. 生成关键字和哈希
    const keywords = await invoke<KeywordToken[]>('tokenize_and_count', {
      plaintext: currentDiaryContent.value
    });

    // 2. 加密日记内容
    const ed = await invoke<EncryptData>('encrypt_data', {
      dek: store.dek,
      plaintext: currentDiaryContent.value,
    });

    // 3. 构建索引批次
    const batchIndexes = [] as BatchIndexEntry[];
    for (const keyword of keywords) {
      const search_hash = await invoke<number[]>('generate_search_hash', {
        dek: store.dek,
        keyword: keyword.word,
      });
      batchIndexes.push({
        id,
        search_hash: search_hash,
        count: keyword.count,
      });
    }

    // 4. 保存本地索引（会删除旧索引）
    await invoke('save_keyword_index_batch', {
      entries: batchIndexes,
    });

    // 5. 更新本地追踪器 (待实现)
    // 这一步是同步逻辑的关键，我们先留空
    // await invoke('update_sync_tracker', { id, encHash: ed.enc_hash });

    // 6. 上传文件到 OSS
    store.statusMessage = '正在上传到 OSS...';
    const objectKey = `${id}.dat`;

    await uploadDiaryFile(objectKey, {
      totalLength: ed.total_length,
      algorithm: ed.algorithm,
      nonce: ed.nonce,
      encHash: ed.enc_hash,
    }, ed.ciphertext);
    console.log("Uploaded encrypted data for ID:", id);

    // 7. 更新本地状态
    store.statusMessage = "保存成功！";
    store.currentEntryId = null;
    currentDiaryContent.value = '';
    await store.loadDiaryList(); // 重新加载列表以显示新 ID
    store.viewMode = 'list';

  } catch (e) {
    store.statusMessage = `保存失败: ${e}`;
    console.error(e);
  }
}

async function handleSearch() {
  if (store.dek.length !== 32 || !searchKeyword.value) return;
  store.statusMessage = '正在搜索...';

  try {
    const search_hash = await invoke<number[]>('generate_search_hash', {
      dek: store.dek,
      keyword: searchKeyword.value.trim(),
    });

    // 1. 查询本地索引
    const matchedEntries = await invoke<SearchIndexResult[]>('search_local_index', {
      searchHash: search_hash,
    });

    if (matchedEntries.length === 0) {
      store.statusMessage = `未找到匹配项。`;
      searchResults.value = [];
      return;
    }

    store.statusMessage = `找到 ${matchedEntries.length} 条，正在下载解密...`;
    const decryptedResults = [] as PageSearchResult[];

    // 2. 遍历下载和解密
    for (const index of matchedEntries) {
      const objectKey = `${index.id}.dat`;

      // 下载
      const head = await downloadFileHead(objectKey);
      const fullEncryptedData = await downloadFile(objectKey);
      const nonceBytes = head.nonce;

      // 提取密文（跳过头部，以及重复的 12 字节 IV）
      const ciphertext = fullEncryptedData.slice(head.totalLength);

      const plaintext = await invoke<string>('decrypt_data', {
        dek: store.dek,
        ciphertext,
        nonceBytes,
      });

      decryptedResults.push({
        id: index.id,
        content: plaintext,
      });
    }

    searchResults.value = decryptedResults;
    store.statusMessage = `搜索完成。`;

  } catch (error) {
    store.statusMessage = `搜索失败: ${error}`;
    searchResults.value = [];
  }
}

async function handleEntryClick(entry: DiaryEntry) {
  if (!store.dek.length) return;

  store.statusMessage = `正在下载 ID: ${entry.id}...`;
  store.viewMode = 'editor';
  store.currentEntryId = entry.id;
  currentDiaryContent.value = '加载中...';

  try {
    const objectKey = `${entry.id}.dat`;

    // 下载
    const head = await downloadFileHead(objectKey);
    const fullEncryptedData = await downloadFile(objectKey);

    const nonceBytes = head.nonce;
    // 提取密文（跳过头部，以及重复的 12 字节 IV）
    const ciphertext = fullEncryptedData.slice(head.totalLength);

    currentDiaryContent.value = await invoke<string>('decrypt_data', {
      dek: store.dek,
      ciphertext,
      nonceBytes,
    });
    store.statusMessage = `加载成功`;

  } catch (e) {
    store.statusMessage = `加载失败: ${e}`;
    currentDiaryContent.value = '';
    console.error(e);
  }
}

// 暂不实现删除，留待同步功能完成后
// async function handleDeleteDiary(id: number) {}
</script>

<template>
  <div class="app-panel">
    <div v-if="store.viewMode === 'list'" class="list-view">
      <div class="list-actions">
        <button @click="store.openNewEntry" class="primary-btn">+ 新建日记</button>
      </div>

      <div class="diary-list">
        <div v-for="item in store.diaryList" :key="item.id" class="diary-item" @click="handleEntryClick(item)">
          <span class="date">{{ new Date(item.id).toLocaleString() }}</span>
          <span class="id-preview">ID: {{ item.id }}</span>
        </div>
        <p v-if="store.diaryList.length === 0" style="color:#999">暂无日记</p>
      </div>
    </div>

    <div v-else class="editor-view">
      <div class="search-box">
        <input v-model="searchKeyword" placeholder="输入关键词搜索..."/>
        <button @click="handleSearch">搜索</button>
      </div>

      <div v-if="searchResults.length > 0" class="search-results">
        <div v-for="result in searchResults" :key="result.id" class="result-card">
          <small>{{ new Date(result.id).toLocaleString() }}</small>
          <p style="white-space: pre-wrap;">{{ result.content }}</p>
        </div>
      </div>

      <hr/>

      <h3>{{ isNewEntry ? '新建日记' : '编辑/查看' }}</h3>
      <textarea v-model="currentDiaryContent" rows="12" placeholder="写点什么..."></textarea>
      <button @click="handleSaveDiary" class="primary-btn">加密上传</button>

      <p class="status">{{ store.statusMessage }}</p>
    </div>
  </div>
</template>

<style scoped>
/* TODO 优化样式管理并重构页面布局(现在的有点丑了), 方便未来引入暗色模式 */
.app-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.list-actions {
  display: flex;
  gap: 10px;
  margin-bottom: 20px;
}

.list-actions button {
  flex-grow: 1;
}

textarea {
  padding: 10px;
  border: 1px solid #ddd;
  border-radius: 4px;
  width: 100%;
  box-sizing: border-box;
  margin-bottom: 10px;
}

input {
  padding: 10px;
  border: 1px solid #ddd;
  border-radius: 4px;
  width: 61%;
  box-sizing: border-box;
  margin-bottom: 10px;
}

.primary-btn {
  background: #396cd8;
  color: white;
  width: 100%;
}

.search-box {
  display: flex;
  gap: 10px;
}

.search-box input {
  margin-bottom: 0;
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

.status {
  color: #666;
  font-size: 0.9rem;
  margin-top: 10px;
}
</style>