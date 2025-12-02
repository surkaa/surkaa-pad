<script setup lang="ts">
import {computed, onMounted, onUnmounted, ref} from "vue";
import {DiaryManifest} from "../types";
import {invoke} from "@tauri-apps/api/core";
import {useRouter} from "vue-router";
import {formatTimestamp} from "../utils/time.ts";
import {parseHtmlToText, parseTextToHtml} from "../utils/diaryParser.ts";

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

// observer 实例
let observer: MutationObserver | null = null;
const diary = ref<DiaryManifest>(DEFAULT_DIARY);
const saveLoading = ref(false);
const delLoading = ref(false);
const isNew = computed(() => !diary.value.id); // 判断是否为新建日记

// 编辑器 DOM 引用
const editorRef = ref<HTMLElement | null>(null);
// 文件输入框引用
const fileInputRef = ref<HTMLInputElement | null>(null);

const contentLen = computed(() => {
  return diary.value.content ? diary.value.content.length : 0;
});

function initObserver() {
  if (!editorRef.value) return;

  observer = new MutationObserver((mutations) => {
    mutations.forEach((mutation) => {
      // 检测是否有节点被移除
      if (mutation.removedNodes.length > 0) {
        mutation.removedNodes.forEach((node) => {
          // 判断移除的是否是我们的图片
          if (node.nodeName === 'IMG' && (node as Element).classList.contains('diary-img')) {
            const imgNode = node as HTMLImageElement;
            const filename = imgNode.dataset.filename;
            console.log(`检测到图片被移除: ${filename}`);
            // TODO 暂时不自动删除附件，等保存日记时统一处理
          }
        });
      }
    });
  });

  // 开始监听 editorRef 的子节点变化
  observer.observe(editorRef.value, {
    childList: true, // 监听子节点增删
    subtree: true    // 监听所有后代节点（防止图片嵌套在 div 里被一起删掉）
  });
}

// 返回上一级页面
function back() {
  // TODO: 提示保存未保存的更改？
  router.back();
}

// 保存或者更新日记
async function saveDiary() {
  saveLoading.value = true;
  if (!editorRef.value) return;

  const currentImages = Array.from(editorRef.value.querySelectorAll('img.diary-img'))
      .map(img => (img as HTMLElement).dataset.filename)
      .filter(name => name !== undefined) as string[];

  // 从 DOM 解析回纯文本 + 标记
  diary.value.content = parseHtmlToText(editorRef.value);

  if (!diary.value.content || diary.value.content.length === 0) {
    alert("日记内容不能为空");
    saveLoading.value = false;
    return;
  }

  try {

    // 找出原有附件列表中，现在已经不存在于编辑器里的文件
    if (!isNew.value && diary.value.attachments) {
      const filesToDelete = diary.value.attachments.filter(att => {
        // 如果附件在当前编辑器里找不到，说明被删了
        return !currentImages.includes(att.filename);
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
      alert("日记保存成功");
    } else {
      // 更新日记
      console.log("更新日记, Old Diary: ", diary.value);
      const d = await invoke<DiaryManifest>("update_diary_content_only", {
        uuid: diary.value.id,
        newContent: diary.value.content
      });
      diary.value = d;
      console.log("日记更新成功, Diary: ", d);
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
    await invoke("delete_diary", {uuid: diary.value.id});
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

// 触发添加图片
function triggerAddImage() {
  fileInputRef.value?.click();
}

// 处理图片选择与上传
async function handleImageSelect(event: Event) {
  if (isNew.value) {
    alert("请先保存一次日记再上传图片（需要生成日记ID）");
    return;
  }

  const input = event.target as HTMLInputElement;
  if (!input.files || input.files.length === 0) return;

  const file = input.files[0];
  // 简单的 MIME 类型检查
  if (!file.type.startsWith("image/")) {
    alert("请选择图片文件");
    return;
  }

  console.log("选择的图片文件: ", file);

  try {
    // 读取文件字节
    const arrayBuffer = await file.arrayBuffer();
    const bytes = Array.from(new Uint8Array(arrayBuffer));

    // 调用后端上传
    const updatedManifest = await invoke<DiaryManifest>("add_attachment", {
      uuid: diary.value.id,
      bytes,
      minetype: file.type
    });

    // 找出新增加的文件名
    // 比较新旧 attachments 列表，找到多出来的那个
    const oldFiles = diary.value.attachments.map(a => a.filename);
    const newFile = updatedManifest.attachments.find(a => !oldFiles.includes(a.filename));

    if (!newFile) {
      // throw new Error("无法获取新上传的文件名");
      alert("上传成功，但无法获取新上传的文件名");
      return;
    }

    // 更新本地数据
    diary.value = updatedManifest;

    // 在光标位置插入图片
    insertImageToEditor(file, newFile.filename);

  } catch (e) {
    console.error("上传图片失败", e);
    alert("上传图片失败: " + e);
  } finally {
    input.value = "";
  }
}

// 将图片插入到编辑器光标处
function insertImageToEditor(file: File, filename: string) {
  if (!editorRef.value) return;

  // 临时的 Blob URL 用于显示
  const url = URL.createObjectURL(file);

  // 创建 img 标签
  const img = document.createElement('img');
  img.src = url;
  img.className = 'diary-img'; // 对应 CSS 样式
  img.dataset.filename = filename; // 重要：存下文件名
  // 设置为块级元素
  img.style.display = 'block';
  img.style.width = '100%';
  img.style.marginTop = '10px';
  img.style.marginBottom = '10px';

  editorRef.value.focus();

  // 获取选区
  const selection = window.getSelection();
  if (selection && selection.rangeCount > 0) {
    const range = selection.getRangeAt(0);

    // 检查选区是否在编辑器内
    if (editorRef.value.contains(range.commonAncestorContainer)) {
      range.deleteContents();
      range.insertNode(img);
      range.setStartAfter(img);
      range.setEndAfter(img);
      range.collapse(true);

      selection.removeAllRanges();
      selection.addRange(range);
    } else {
      // 如果光标不在编辑器里，追加到最后
      editorRef.value.appendChild(img);
    }
  } else {
    editorRef.value.appendChild(img);
  }
}

onMounted(async () => {
  if (history.state.diary) {
    diary.value = history.state.diary;

    // 将纯文本转为 HTML (带图片)
    if (editorRef.value) {
      editorRef.value.innerHTML = await parseTextToHtml(
          diary.value.content,
          diary.value.id,
          diary.value.attachments
      );
    }
  }
  initObserver();
});

onUnmounted(() => {
  if (observer) {
    observer.disconnect();
    observer = null;
  }
})
</script>

<template>
  <main id="diary-detail">
    <section id="diary-detail-header">
      <button id="diary-detail-header-back-btn" @click="back">返回</button>
      <button @click="triggerAddImage" :disabled="isNew">
        插入图片 {{ isNew ? '(需先保存)' : '' }}
      </button>
      <input
          type="file"
          ref="fileInputRef"
          style="display: none"
          accept="image/*"
          @change="handleImageSelect"
      />
      <button id="diary-detail-header-save-btn" @click="saveDiary" :disabled="saveLoading">
        {{ saveLoading ? "保存中..." : (isNew ? "保存日记" : "更新日记") }}
      </button>
      <button id="diary-detail-header-delete-btn" @click="deleteDiary" :disabled="delLoading">
        {{ delLoading ? "删除中..." : "删除日记" }}
      </button>
    </section>
    <section id="diary-detail-main">
      <div
          id="diary-editor"
          ref="editorRef"
          contenteditable="true"
          class="custom-editor"
          spellcheck="false"
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
  display: flex;
  flex-direction: column;

  #diary-detail-header {
    height: 50px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px;
    background-color: var(--pad-bg-color-300);
    border-bottom: 1px solid var(--pad-border-color-400);

    button {
      padding: 5px 10px;
      font-size: 14px;
      cursor: pointer;
    }
  }

  #diary-detail-main {
    flex: 1;
    padding: 10px;
    background-color: var(--pad-bg-color-100);

    .custom-editor {
      width: calc(100% - 4 * $diary-editor-padding);
      min-height: calc(100% - 4 * $diary-editor-padding);
      outline: none;
      overflow-y: auto;
      white-space: pre-wrap;
      word-wrap: break-word;
      padding: $diary-editor-padding;
      background-color: var(--pad-bg-color-100);
      color: var(--pad-text-color-100);
      font-family: inherit;
      border: 1px solid var(--pad-border-color-100);
      text-align: left;

      :deep(img.diary-img) {
        display: block;
        width: 100%;
        margin: 10px 0;
        border-radius: 4px;
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
        cursor: default;

        &:hover {
          box-shadow: 0 0 0 2px var(--pad-shadow-color-100); /* 选中效果 */
        }
      }
    }
  }

  #diary-detail-footer {
    height: 40px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 10px;
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
