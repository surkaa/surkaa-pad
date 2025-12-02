<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { DiaryManifest, AttachmentMeta } from "../types";
import { invoke } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";
import {formatTimestamp} from "../utils/time.ts";

const router = useRouter();

// 默认值用于新建日记
const defaultDiary: DiaryManifest = {
  id: "",
  content: "",
  created: Date.now(),
  updated: Date.now(),
  algorithm: "AES-256-GCM", // 默认加密算法
  attachments: []
};

const diary = ref<DiaryManifest>(defaultDiary);
const saveLoading = ref(false);
const delLoading = ref(false);
const isNew = computed(() => !diary.value.id); // 判断是否为新建日记

const contentLen = computed(() => {
  return diary.value.content ? diary.value.content.length : 0;
});

// 返回上一级页面
function back() {
  router.back();
}

// 保存或者更新日记
async function saveDiary() {
  saveLoading.value = true;
  if (!diary.value.content || diary.value.content.length === 0) {
    alert("日记内容不能为空");
    saveLoading.value = false;
    return;
  }
  try {
    if (isNew.value) {
      // 新建日记
      console.log("新建日记", diary.value);
      const id = await invoke<string>("save_diary", {
        content: diary.value.content
      });
      diary.value.id = id;
      // 模拟后端更新时间
      diary.value.created = Date.now();
      diary.value.updated = Date.now();
      console.log("日记保存成功, ID:", id);
      alert("日记保存成功");
    } else {
      // 更新日记
      console.log("更新日记", diary.value);
      await invoke("update_diary_content_only", {
        uuid: diary.value.id,
        newContent: diary.value.content
      });
      diary.value.updated = Date.now(); // 模拟后端更新时间
      console.log("日记更新成功");
      alert("日记更新成功");
    }
  } catch (e) {
    console.error("保存日记失败", e);
    alert("保存日记失败: " + e);
  } finally {
    saveLoading.value = false;
  }
}

// 删除当前日记
async function deleteDiary() {
  if (isNew.value) {
    const confirmAbandon = confirm("当前日记未保存, 确认放弃并返回吗?");
    if (confirmAbandon) back();
    return;
  }

  const confirmDelete = confirm("⚠️ 确认永久删除这篇日记吗?");
  if (!confirmDelete) return;

  delLoading.value = true;
  try {
    await invoke("delete_diary", { uuid: diary.value.id });
    console.log("日记删除成功");
    alert("日记删除成功");
    back();
  } catch (e) {
    console.error("删除日记失败", e);
    alert("删除日记失败: " + e);
  } finally {
    delLoading.value = false;
  }
}

// ----------------------------------------------------
// 附件管理函数
// ----------------------------------------------------

// 模拟文件选择和读取，然后调用添加附件
async function handleAttachFile(event: Event) {
  if (isNew.value) {
    alert("请先保存日记内容后再添加附件！");
    return;
  }

  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;

  // 模拟文件读取 (在实际 Tauri 应用中，可能需要更安全的 FS 操作)
  // 这里简化为 base64 或 ArrayBuffer 转换
  try {
    // 假设我们能获取文件的字节数据 (Vec<u8>) 和 MIME 类型
    const fileBuffer = await file.arrayBuffer(); // 假设这是字节数据
    const mimeType = file.type || 'application/octet-stream';
    const bytes = Array.from(new Uint8Array(fileBuffer));

    console.log(`添加附件: ${file.name}, 类型: ${mimeType}, 大小: ${file.size}`);

    // 调用后端添加附件命令
    await invoke("add_attachment", {
      uuid: diary.value.id,
      bytes: bytes,
      minetype: mimeType
    });

    // 成功后，需要后端返回新的附件列表或新附件的元数据
    // 这里的实现是简化的，假设我们知道文件名
    const newAttachment: AttachmentMeta = {
      filename: file.name, // 实际应用中文件名可能是哈希ID
      mimetype: mimeType,
      size: file.size,
      nonce: [] // 实际需要后端返回的 nonce
    };
    diary.value.attachments.push(newAttachment);
    input.value = ''; // 清空 input 以便再次上传同一文件
    alert(`附件 "${file.name}" 添加成功！`);

  } catch (e) {
    console.error("添加附件失败:", e);
    alert("添加附件失败。");
  }
}

// 下载附件
async function handleDownloadAttachment(attachment: AttachmentMeta) {
  try {
    const fileBytes = await invoke<number[]>("download_attachment", {
      uuid: diary.value.id,
      filename: attachment.filename,
      nonce: attachment.nonce
    });

    // 模拟文件下载 (将字节数据转换为 Blob 并下载)
    const blob = new Blob([new Uint8Array(fileBytes)], { type: attachment.mimetype });
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = attachment.filename;
    document.body.appendChild(a);
    a.click();
    window.URL.revokeObjectURL(url);
    a.remove();

    console.log(`附件 "${attachment.filename}" 下载成功`);

  } catch (e) {
    console.error("下载附件失败:", e);
    alert("下载附件失败: " + e);
  }
}

// 删除附件
async function handleDeleteAttachment(attachment: AttachmentMeta) {
  if (!confirm(`确认删除附件 "${attachment.filename}" 吗?`)) return;

  try {
    await invoke("delete_attachment", {
      uuid: diary.value.id,
      filename: attachment.filename
    });

    // 从本地列表中移除
    diary.value.attachments = diary.value.attachments.filter(
        (a) => a.filename !== attachment.filename
    );
    alert(`附件 "${attachment.filename}" 删除成功`);

  } catch (e) {
    console.error("删除附件失败:", e);
    alert("删除附件失败: " + e);
  }
}

onMounted(() => {
  console.log('Diary OnMounted');
  // 检查路由状态中是否有日记对象，用于编辑现有日记
  if (history.state.diary) {
    diary.value = history.state.diary;
  }
});
</script>

<template>
  <main id="diary-detail">
    <header class="detail-header">
      <button class="icon-btn back-btn" @click="back" title="返回">
        &larr;
      </button>

      <h1 class="title">{{ isNew ? '新建日记' : '编辑日记' }}</h1>

      <div class="actions">
        <button
            class="btn danger-btn"
            @click="deleteDiary"
            :disabled="delLoading"
        >
          {{ isNew ? '放弃' : (delLoading ? '删除中...' : '删除') }}
        </button>

        <button
            class="btn primary-btn"
            @click="saveDiary"
            :disabled="saveLoading"
        >
          {{ saveLoading ? '保存中...' : '保存' }}
        </button>
      </div>
    </header>

    <hr />

    <section class="content-area">
      <textarea
          v-model="diary.content"
          placeholder="记录你的一天..."
          autofocus
      ></textarea>

      <div class="metadata">
        <span>长度: {{ contentLen }}</span>
        <span v-if="!isNew">创建于: {{ formatTimestamp(diary.created) }}</span>
        <span v-if="!isNew">加密算法: {{ diary.algorithm }}</span>
      </div>
    </section>

    <section class="attachment-area">
      <h2>附件 ({{ diary.attachments.length }})</h2>
      <ul class="attachment-list">
        <li v-for="att in diary.attachments" :key="att.filename" class="attachment-item">
          <span class="file-name">{{ att.filename }} ({{ (att.size / 1024).toFixed(2) }} KB)</span>
          <div class="att-actions">
            <button class="btn info-btn small-btn" @click="handleDownloadAttachment(att)">下载</button>
            <button class="btn danger-btn small-btn" @click="handleDeleteAttachment(att)">删除</button>
          </div>
        </li>

        <li class="attachment-add">
          <label for="file-upload" class="btn primary-btn small-btn" :class="{ 'disabled': isNew }">
            + 添加附件
          </label>
          <input
              id="file-upload"
              type="file"
              @change="handleAttachFile"
              :disabled="isNew"
              style="display: none;"
          />
        </li>
      </ul>
    </section>
  </main>
</template>

<style scoped lang="scss">
// 使用 SCSS 和 CSS 变量进行配色和布局

#diary-detail {
  width: 100%;
  max-width: 900px;
  height: 100%;
  padding: 1rem;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  background-color: var(--pad-bg-color-100);
}

.detail-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 0;

  .title {
    flex-grow: 1;
    text-align: center;
    margin: 0;
    font-size: 1.5rem;
    color: var(--pad-text-color-100);
  }

  .icon-btn {
    background: none;
    border: none;
    font-size: 1.8rem;
    cursor: pointer;
    color: var(--pad-text-color-300);
    padding: 0 10px;
    transition: color 0.2s;

    &:hover {
      color: var(--pad-primary-color);
    }
  }

  .actions {
    display: flex;
    gap: 10px;
  }
}

hr {
  margin: 10px 0;
}

// 按钮通用样式
.btn {
  padding: 8px 15px;
  border-radius: 6px;
  border: none;
  cursor: pointer;
  font-weight: 500;
  transition: all 0.2s;

  &:disabled, &.disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
}

.primary-btn {
  background-color: var(--pad-primary-color);
  color: var(--pad-bg-color-100); // 确保在浅色背景上文字清晰
  &:hover:not(:disabled) {
    background-color: darken(#C4A484, 10%); // 木色加深
  }
}

.danger-btn {
  background-color: var(--pad-danger-color);
  color: var(--pad-bg-color-100);
  &:hover:not(:disabled) {
    background-color: darken(#D9534F, 10%); // 红色加深
  }
}

.info-btn {
  background-color: var(--pad-info-color);
  color: var(--pad-bg-color-100);
  &:hover:not(:disabled) {
    background-color: darken(#ADB5BD, 10%); // 灰色加深
  }
}

.small-btn {
  padding: 5px 10px;
  font-size: 0.8rem;
}


// 内容区域
.content-area {
  flex-grow: 1;
  display: flex;
  flex-direction: column;
  padding-bottom: 10px;

  textarea {
    flex-grow: 1;
    width: 100%;
    min-height: 200px;
    padding: 20px;
    box-sizing: border-box;
    border: 1px solid var(--pad-border-color-200);
    border-radius: 8px;
    resize: none;
    font-size: 1rem;
    line-height: 1.6;
    font-family: inherit;
    background-color: var(--pad-bg-color-200);
    color: var(--pad-text-color-100);
    transition: border-color 0.3s, background-color 0.3s;

    &:focus {
      outline: none;
      border-color: var(--pad-border-color-300);
      box-shadow: 0 0 0 1px var(--pad-border-color-300);
    }
  }

  .metadata {
    display: flex;
    justify-content: flex-end;
    gap: 20px;
    font-size: 0.8rem;
    color: var(--pad-text-color-400);
    padding-top: 5px;
  }
}

// 附件区域
.attachment-area {
  margin-top: 20px;

  h2 {
    font-size: 1.1rem;
    color: var(--pad-text-color-200);
    border-left: 3px solid var(--pad-primary-color);
    padding-left: 10px;
    margin-bottom: 10px;
  }

  .attachment-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .attachment-item, .attachment-add {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-radius: 6px;
    background-color: var(--pad-bg-color-300);
    border: 1px solid var(--pad-border-color-200);
    transition: background-color 0.2s;

    .file-name {
      font-size: 0.9rem;
      color: var(--pad-text-color-300);
      margin-right: 15px;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .att-actions {
      display: flex;
      gap: 5px;
    }
  }

  .attachment-add {
    .btn {
      margin: 0;
    }
    .disabled {
      background-color: var(--pad-bg-color-400);
      color: var(--pad-text-color-500);
    }
  }
}
</style>