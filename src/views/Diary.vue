<script setup lang="ts">
import {computed, onMounted, onUnmounted, ref} from "vue";
import {DiaryManifest} from "../types";
import {invoke} from "@tauri-apps/api/core";
import {useRouter} from "vue-router";
import {formatTimestamp} from "../utils/time.ts";
import {parseHtmlToText, parseTextToHtml} from "../utils/diaryParser.ts";
import {writeFile, BaseDirectory} from '@tauri-apps/plugin-fs';
import {showToast} from "../utils/toast.ts";

const router = useRouter();

// 默认值用于新建日记
const DEFAULT_DIARY: DiaryManifest = {
  id: "",
  content: "",
  created: Date.now(),
  updated: Date.now(),
  algorithm: "AES-256-GCM", // 默认加密算法
  attachments: []
} as const;

const diary = ref<DiaryManifest>(DEFAULT_DIARY);
const renderLoading = ref(false);
const saveLoading = ref(false);
const delLoading = ref(false);
const uploadLoading = ref(false);
const isNew = computed(() => !diary.value.id); // 判断是否为新建日记
const keyword = ref('');

// 编辑器 DOM 引用
const editorRef = ref<HTMLElement | null>(null);
// 文件输入框引用
const fileInputRef = ref<HTMLInputElement | null>(null);
const showMediaMenu = ref(false);
const waitToUnlistedSet = new Set<{ fn: (() => void) | null, eid: string | null }>();

function toggleMediaMenu() {
  // 切换菜单显示状态
  showMediaMenu.value = !showMediaMenu.value;
}

// 媒体选择后的通用处理函数 (用于关闭菜单)
function mediaSelected() {
  showMediaMenu.value = false;
}

// 触发图片选择
function triggerAddImage() {
  if (isNew.value) { /* 提醒逻辑 */
    return;
  }
  if (fileInputRef.value) {
    fileInputRef.value.accept = 'image/*';
    fileInputRef.value.click();
  }
  mediaSelected(); // 关闭菜单
}

// 触发视频选择
function triggerAddVideo() {
  if (isNew.value) { /* 提醒逻辑 */
    return;
  }
  if (fileInputRef.value) {
    fileInputRef.value.accept = 'video/*';
    fileInputRef.value.click();
  }
  mediaSelected(); // 关闭菜单
}

// 触发录音选择
function triggerAddAudio() {
  if (isNew.value) { /* 提醒逻辑 */
    return;
  }
  if (fileInputRef.value) {
    fileInputRef.value.accept = 'audio/*';
    fileInputRef.value.click();
  }
  mediaSelected(); // 关闭菜单
}

const contentLen = computed(() => {
  return diary.value.content ? diary.value.content.length : 0;
});

// 返回上一级页面 之所以用replace是为了传递state而不是query参数
function back(needRefresh = false) {
  // 如果需要强制刷新(通常是删除了日记)，直接返回，不需检查
  if (needRefresh) {
    router.replace({
      name: "DiaryList",
      state: { refresh: true, keyword: keyword.value }
    });
    return;
  }

  // 获取当前的逻辑内容（将 HTML 解析回存储格式）
  let currentContent = "";
  if (editorRef.value) {
    currentContent = parseHtmlToText(editorRef.value);
  }

  console.log("返回检查:", {
    isNew: isNew.value,
    savedLen: diary.value.content?.length || 0,
    currentLen: currentContent.length,
    changed: currentContent !== diary.value.content
  });

  // 场景 1: 新建日记 (原有逻辑优化)
  // 如果是新建且有内容，提示保存
  if (isNew.value && currentContent.length > 0) {
    const confirmLeave = confirm("新建日记尚未保存，确认返回？未保存的内容将会丢失。");
    if (!confirmLeave) return;
  }

  // 场景 2: 已有日记 (新增功能)
  // 如果是已有日记，对比当前解析后的内容与内存中(上次保存/加载)的内容
  if (!isNew.value) {
    // 注意：这里假设 diary.value.content 始终保持为最后一次 save 或 load 的状态
    if (currentContent !== diary.value.content) {
      const confirmLeave = confirm("当前日记有未保存的更改，确认直接返回吗？更改将不会被保存。");
      if (!confirmLeave) return;
    }
  }

  // 通过检查，执行路由跳转
  router.replace({
    name: "DiaryList",
    state: { refresh: false, keyword: keyword.value }
  });
}

// 保存或者更新日记
async function saveDiary() {
  saveLoading.value = true;
  if (!editorRef.value) return;

  const currentMedias = Array.from(editorRef.value.querySelectorAll('.diary-media'))
      .map(el => (el as HTMLElement).dataset.filename)
      .filter(fn => fn) as string[];

  // 从 DOM 解析回纯文本 + 标记
  diary.value.content = parseHtmlToText(editorRef.value);

  if (!diary.value.content || diary.value.content.length === 0) {
    showToast("日记内容不能为空", 'warning');
    saveLoading.value = false;
    return;
  }

  try {

    // 找出原有附件列表中，现在已经不存在于编辑器里的文件
    if (!isNew.value && diary.value.attachments) {
      const filesToDelete = diary.value.attachments.filter(att => {
        // 如果附件在当前编辑器里找不到，说明被删了
        return !currentMedias.includes(att.filename);
      });

      if (filesToDelete.length > 0) {
        console.log("检测到孤儿附件，准备清理:", filesToDelete);

        // 并行调用删除接口 必须删完再保存 保持数据一致性

        await Promise.all(filesToDelete.map(att =>
            invoke("delete_attachment", {
              uuid: diary.value.id,
              filename: att.filename
            })
        ));
      }
    }

    if (isNew.value) {
      // 新建日记
      console.log("新建日记", diary.value);
      const d = await invoke<DiaryManifest>("save_diary", {
        content: diary.value.content
      });
      diary.value = d;
      console.log("日记保存成功, Diary: ", d);
      showToast("日记保存成功", 'success');
    } else {
      // 更新日记
      console.log("更新日记, Old Diary: ", diary.value);
      const d = await invoke<DiaryManifest>("update_diary_content_only", {
        uuid: diary.value.id,
        newContent: diary.value.content
      });
      diary.value = d;
      console.log("日记更新成功, Diary: ", d);
      showToast('日记更新成功');
    }
  } catch (e) {
    console.error("保存日记失败", e);
    showToast('保存日记失败: ' + e, 'error');
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
    await invoke("delete_diary", {uuid: diary.value.id});
    console.log("日记删除成功");
    back(true);
    showToast('日记删除成功', 'success');
  } catch (e) {
    console.error("删除日记失败", e);
    showToast("删除日记失败: " + e, 'error');
  } finally {
    delLoading.value = false;
  }
}

// 根据文件类型确定 Marker 前缀
function getTagPrefix(mimeType: string): 'IMG' | 'VID' | 'AUD' | null {
  if (mimeType.startsWith('image/')) return 'IMG';
  if (mimeType.startsWith('video/')) return 'VID';
  if (mimeType.startsWith('audio/')) return 'AUD';
  return null;
}

// 处理图片选择与上传
async function handleMediaSelect(event: Event) {
  const input = event.target as HTMLInputElement;
  if (!input.files || input.files.length === 0) return;

  if (isNew.value) {
    showToast("请先保存一次日记再上传图片（需要生成日记ID）", 'info');
    input.value = "";
    return;
  }

  const file = input.files[0];
  const tagPrefix = getTagPrefix(file.type);

  if (!tagPrefix) {
    showToast("不支持的文件类型: " + file.type, 'error');
    input.value = "";
    return;
  }

  console.log("选择的图片文件: ", file);

  try {
    uploadLoading.value = true;
    // 读取文件字节
    const arrayBuffer = await file.arrayBuffer();
    const bytes = new Uint8Array(arrayBuffer);

    // 构造临时文件名/路径
    const tempFilename = `${diary.value.id}_${Date.now()}_${file.name}`;

    console.log("准备上传文件到临时路径: ", tempFilename);

    // 将文件写入应用数据目录或临时目录
    await writeFile(tempFilename, bytes, {
      baseDir: BaseDirectory.Temp
    });

    // 调用后端上传
    const updatedManifest = await invoke<DiaryManifest>("add_attachment", {
      uuid: diary.value.id,
      filename: tempFilename,
      minetype: file.type
    });

    // 找出新增加的文件名
    // 比较新旧 attachments 列表，找到多出来的那个
    const oldFiles = diary.value.attachments.map(a => a.filename);
    const newFile = updatedManifest.attachments.find(a => !oldFiles.includes(a.filename));

    if (!newFile) {
      // throw new Error("无法获取新上传的文件名");
      showToast("上传成功，但无法获取新上传的文件名", 'success');
      return;
    }

    // 更新本地数据
    diary.value = updatedManifest;

    // 在光标位置插入图片
    insertImageToEditor(file, newFile.filename, tagPrefix);

    // 自动更新保存日记
    await saveDiary();
  } catch (e) {
    console.error("上传图片失败", e);
    showToast("上传图片失败: " + e, 'error');
  } finally {
    input.value = "";
    uploadLoading.value = false;
  }
}

// 将图片插入到编辑器光标处
function insertImageToEditor(file: File, filename: string, tagPrefix: 'IMG' | 'VID' | 'AUD') {
  if (!editorRef.value) return;

  // 临时的 Blob URL 用于显示
  const url = URL.createObjectURL(file);
  let mediaElement: HTMLImageElement | HTMLVideoElement | HTMLAudioElement;

  // 1. 创建元素
  if (tagPrefix === 'IMG') {
    mediaElement = document.createElement('img');
  } else if (tagPrefix === 'VID') {
    mediaElement = document.createElement('video');
    mediaElement.setAttribute('controls', 'true');
    mediaElement.style.display = 'block';
    mediaElement.style.width = '100%';
  } else if (tagPrefix === 'AUD') {
    mediaElement = document.createElement('audio');
    mediaElement.setAttribute('controls', 'true');
  } else {
    console.log("不支持的媒体类型: ", tagPrefix);
    return; // 不支持的类型
  }

  // 2. 设置通用属性
  mediaElement.src = url;
  mediaElement.className = `diary-media ${tagPrefix.toLowerCase()}`;
  mediaElement.dataset.filename = filename;
  mediaElement.style.marginTop = '10px';
  mediaElement.style.marginBottom = '10px';

  editorRef.value.focus();

  // 获取选区
  const selection = window.getSelection();
  if (selection && selection.rangeCount > 0) {
    const range = selection.getRangeAt(0);
    range.deleteContents();
    range.insertNode(mediaElement);
    // 插入换行符并将光标移到其后
    const br = document.createElement('br');
    range.setStartAfter(mediaElement);
    range.insertNode(br);
    range.setStartAfter(br);
    range.collapse(true);
    selection.removeAllRanges();
    selection.addRange(range);
  } else {
    editorRef.value.appendChild(mediaElement);
  }
}

onMounted(async () => {
  if (history.state.keyword) {
    keyword.value = history.state.keyword;
  }
  if (history.state.diary) {
    diary.value = history.state.diary;

    // 将纯文本转为 HTML (带图片)
    if (editorRef.value) {
      try {
        renderLoading.value = true;
        let content = diary.value.content;
        const results = await parseTextToHtml(
            diary.value.content,
            diary.value.id,
            diary.value.attachments
        );
        for (const res of results) {
          content = content.replace(res.marker, res.html);
          waitToUnlistedSet.add({
            fn: res.unlistedFn,
            eid: res.eid
          });
        }
        // 高亮显示关键词
        if (keyword.value && keyword.value.trim().length > 0) {
          const kw = keyword.value.trim();
          const kwRegex = new RegExp(kw.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
          content = content.replace(kwRegex, (match) => {
            return `<span class="keyword">${match}</span>`;
          });
        }
        editorRef.value.innerHTML = content;
      } finally {
        renderLoading.value = false;
      }
    }
  }
});

onUnmounted(() => {
  // 卸载时调用所有等待的 unlisted 函数
  waitToUnlistedSet.forEach(task => {
    if (task.fn) {
      // 取消监听
      task.fn();
      console.log("取消附件下载监听");
    }
    if (task.eid) {
      // 取消后端下载任务
      invoke("cancel_download_attachment", {eid: task.eid});
      console.log("取消后端附件下载任务, EID=", task.eid);
    }
  });
});
</script>

<template>
  <main id="diary-detail">
    <section id="diary-detail-header">
      <button id="diary-detail-header-back-btn" @click="back()">返回</button>
      <div id="media-menu-container">
        <button
            @click="toggleMediaMenu"
            :disabled="saveLoading || isNew"
        >
          添加 ▼
        </button>

        <div v-if="showMediaMenu" id="media-menu-dropdown">
          <button @click="triggerAddImage">图片</button>
          <button @click="triggerAddVideo">视频</button>
          <button @click="triggerAddAudio">录音</button>
        </div>
      </div>
      <input
          type="file"
          ref="fileInputRef"
          style="display: none"
          accept="image/*"
          @change="handleMediaSelect"
      />
      <button id="diary-detail-header-save-btn" @click="saveDiary" :disabled="saveLoading">
        {{ saveLoading ? "保存中..." : (isNew ? "保存" : "更新") }}
      </button>
      <button id="diary-detail-header-delete-btn" @click="deleteDiary" :disabled="delLoading">
        {{ delLoading ? "删除中..." : "删除" }}
      </button>
    </section>
    <section id="diary-detail-main">
      <div v-if="renderLoading || uploadLoading" id="loading-overlay">
        <p v-if="renderLoading">正在加载日记内容和附件...</p>
        <p v-else-if="uploadLoading">正在上传媒体文件...</p>
      </div>
      <div
          id="diary-editor"
          ref="editorRef"
          contenteditable="true"
          class="custom-editor"
          spellcheck="false"
          :style="{ visibility: renderLoading ? 'hidden' : 'visible' }"
      ></div>
    </section>
    <section id="diary-detail-footer">
      <section id="diary-detail-footer-left">
        <span>字数: {{ contentLen }}</span>
      </section>
      <section id="diary-detail-footer-right">
        <span>最后更新: {{ formatTimestamp(diary.updated) }}</span>
        <span>创建时间: {{ formatTimestamp(diary.created) }}</span>
      </section>
    </section>
  </main>
</template>

<style scoped lang="scss">
$diary-editor-padding: 10px;

#diary-detail {
  width: 100%;
  height: 100%;
  max-height: 100%;
  display: flex;
  flex-direction: column;

  #diary-detail-header {
    height: 50px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 2px 10px;
    background-color: var(--pad-bg-color-300);
    border-bottom: 1px solid var(--pad-border-color-400);
    gap: 10px;

    button {
      padding: 5px 10px;
      font-size: 14px;
      cursor: pointer;
      border-radius: 4px;
      border: 1px solid var(--pad-border-color-200);
    }

    // 第一个按钮靠左，其他靠右
    #diary-detail-header-back-btn {
      margin-right: auto;
    }

    // 删除按钮样式
    #diary-detail-header-delete-btn {
      background-color: var(--pad-danger-color);
      color: white;
    }

    #media-menu-container {
      position: relative; /* 容器相对定位 */
      display: inline-block;

    }

    #media-menu-dropdown {
      position: absolute;
      top: 100%; /* 定位在按钮下方 */
      left: 0;
      z-index: 10; /* 确保菜单位于其他元素之上 */

      /* 菜单外观 */
      background-color: white;
      border: 1px solid #ddd;
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
      min-width: 120px;
      border-radius: 4px;
      padding: 5px 0;

      /* 菜单内按钮布局 */
      button {
        display: block; /* 垂直排列 */
        width: 100%;
        padding: 8px 15px;
        border: none;
        background: none;
        text-align: left;
        cursor: pointer;
        font-size: 14px;
        color: #333;

        &:hover {
          background-color: #f0f0f0;
        }
      }
    }
  }

  #diary-detail-main {
    flex: 1;
    padding: 10px;
    background-color: var(--pad-bg-color-100);
    // 滚动条
    overflow-y: auto;
    position: relative; /* 用于定位蒙版 */

    /* 蒙版样式 */
    #loading-overlay {
      position: absolute;
      top: 0;
      left: 0;
      width: 100%;
      height: 100%;
      z-index: 50; /* 确保高于编辑器内容 */

      /* 蒙版外观：半透明白色/灰色 */
      background-color: rgba(255, 255, 255, 0.9); /* 90% 透明度 */

      /* 内容居中 */
      display: flex;
      justify-content: center;
      align-items: center;

      p {
        color: #333;
        font-weight: bold;
        padding: 10px 20px;
        border-radius: 4px;
        background-color: #f0f0f0; /* 让提示文字更醒目 */
      }
    }

    .custom-editor {
      width: calc(100% - 2 * $diary-editor-padding);
      min-height: calc(100% - 2 * $diary-editor-padding);
      outline: none;
      overflow-y: auto;
      white-space: pre-wrap;
      word-wrap: break-word;
      padding: $diary-editor-padding;
      background-color: var(--pad-bg-color-100);
      color: var(--pad-text-color-100);
      font-family: inherit;
      text-align: left;

      // 泛化媒体样式
      :deep(.diary-media) {
        width: 100%;
        margin: 10px 0;
        border-radius: 4px;
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
        cursor: default;
      }

      // 视频和音频的特殊样式
      :deep(video.diary-media) {
        height: auto;
        background-color: black;
      }

      :deep(audio.diary-media) {
        height: 50px; /* 通常音频文件较短 */
        background-color: transparent;
        box-shadow: unset;
        border-radius: unset;
      }

      // 关键词高亮样式
      :deep(.keyword) {
        background-color: yellow;
        color: black;
      }
    }
  }

  #diary-detail-footer {
    height: 40px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 2px 10px;
    background-color: var(--pad-bg-color-300);
    border-top: 1px solid var(--pad-border-color-400);
  }

  #diary-detail-footer-left, #diary-detail-footer-right {
    display: flex;
    gap: 2rem;
    font-size: 12px;
    color: var(--pad-text-color-300);
  }
}
</style>
