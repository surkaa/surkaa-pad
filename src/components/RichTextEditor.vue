<script setup lang="ts">
// 要实现的功能：
// 渲染内容文本（包括媒体文件）
import {DiaryManifest, DownloadAttachmentEvent} from "../types";
import {computed, onMounted, onUnmounted, ref, watch} from "vue";
import {convertFilename2URL} from "../utils";
import {useAppStore} from "../stores/app.ts";
import {useEventListener} from "../utils/useEventListener.ts";

const appStore = useAppStore();
const model = defineModel<string>({default: ''});
const {
    diary,
    mode
} = defineProps<{
  diary: DiaryManifest;
  mode: 'edit' | 'view'
}>();
const emit = defineEmits<{
  (e: 'update:cursorPosition', position: number): void;
  (e: 'process:downloadAttachment', type: DownloadAttachmentEvent['event'], statusMsg: string): void;
  (e: 'request:previewMedia', eid: string): void;
}>();

const processDownloadAttachment = (type: DownloadAttachmentEvent['event'], statusMsg: string) => {
  emit('process:downloadAttachment', type, statusMsg);
};

const textareaRef = ref<HTMLTextAreaElement | null>(null);
const cancelFns = ref<Function[]>([]);

watch(() => cancelFns.value.length, (newLength) => {
  console.log(`有${newLength}的URL转换待处理.`);
});

const innerHTML = computed(() => {
  let res = model.value.replace(/<<(IMG|VID|AUD):(.+?)>>/g, (match, tag, fn) => {
    const a = diary.attachments.find(att => att.filename === fn);
    const attachmentIndex = diary.attachments.findIndex(a => a.filename === fn);
    if (!a) return match; // 如果找不到附件，返回原始文本

    // 生成eid
    const eid = `EID_${diary.id}_${attachmentIndex}`;
    // 先看看DOM里有没有这个元素，有的话就不重复请求了
    let el = document.getElementById(eid);
    if (el) {
      console.log('元素已存在，直接复用，跳过URL转换');
      // 复制
      if (tag === 'IMG') {
        useEventListener(el, 'click', () => emit('request:previewMedia', eid));
      }
      return el.outerHTML;
    }
    // 请求将文件名转换为URL
    const cFn = convertFilename2URL(diary.id, a.nonce, eid, fn, processDownloadAttachment, (url: string) => {
      // 设置媒体元素的src属性
      const element = document.getElementById(eid) as HTMLMediaElement;
      if (element) {
        // 从cancelFns中移除这个取消函数
        const index = cancelFns.value.indexOf(cFn);
        if (index !== -1) {
          cancelFns.value.splice(index, 1);
        }
        element.src = url;
        // 添加点击事件
        if (tag === 'IMG') {
          useEventListener(element, 'click', () => emit('request:previewMedia', eid));
        }
        processDownloadAttachment("completed", '附件加载完成');
      }
    });
    cancelFns.value.push(cFn);

    if (tag === 'IMG') {
      return `<img class="media-item" id="${eid}" data-filename="${fn}" alt="${fn}" />`;
    } else if (tag === 'VID') {
      return `<video class="media-item" id="${eid}" data-filename="${fn}" controls></video>`;
    } else if (tag === 'AUD') {
      return `<audio class="media-item" id="${eid}" data-filename="${fn}" controls></audio>`;
    }
    console.warn(`未知标签: ${tag}`);
    return match;
  });
  // 只在有搜索关键词时才处理高亮
  if (appStore.keyword) {
    return res.replace(new RegExp(`(${appStore.keyword})`, 'gi'), '<mark>$1</mark>');
  }
  return res;
});
const isEditing = computed(() => mode === 'edit');

/**
 * 获取光标位置并触发事件
 */
function handleCursorPositionChange() {
  if (mode === 'edit' && textareaRef.value) {
    // 获取光标位置 (selectionStart)
    const position = textareaRef.value.selectionStart;

    // 4. 通过 emit 传递给父组件
    emit('update:cursorPosition', position);
  }
}

onMounted(() => {
  console.log('RichTextEditor mounted', mode);
  watch(() => cancelFns.value.length, (newLength) => {
    console.log(`有${newLength}的URL转换待处理.`);
  });
});

onUnmounted(() => {
  // 取消所有未完成的URL转换
  cancelFns.value.forEach(fn => fn());
  // 获取所有url然后释放
  const mediaElements = document.querySelectorAll('.media-item') as NodeListOf<HTMLMediaElement>;
  mediaElements.forEach(el => {
    if (el.src) {
      URL.revokeObjectURL(el.src);
    }
    // 移除点击事件监听器
    el.replaceWith(el.cloneNode(true));
  })
});
</script>

<template>
  <div class="rich-text-editor">
    <textarea
        id="rte-textarea"
        ref="textareaRef"
        class="edit"
        v-show="isEditing"
        v-model="model"
        @input="handleCursorPositionChange"
        @keyup="handleCursorPositionChange"
        @mouseup="handleCursorPositionChange"
    />
    <div class="view" v-show="!isEditing" v-html="innerHTML"/>
  </div>
</template>

<style scoped lang="scss">
.rich-text-editor {
  width: 100%;
  height: 100%;
  text-align: left;
  font-family: var(--pad-font-family),serif;
  font-size: 16px;
  line-height: 1.6;
  color: var(--pad-text-color-100);

  .view {
    width: 100%;
    height: 100%;
    max-width: 100%;
    max-height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    white-space: pre-wrap;
    word-break: break-word;
    padding: 12px;
    line-height: 1.6;
    box-sizing: border-box;
    background-color: var(--pad-bg-color-100);
    border: 1px solid var(--pad-border-color-100);
    transition: all var(--pad-transition-base);

    &:hover {
      border-color: var(--pad-border-color-200);
      box-shadow: var(--pad-shadow-sm);
    }

    &.has-media {
      padding: 20px;
    }

    // 基本文本样式
    & > * {
      margin-bottom: 1em;

      &:last-child {
        margin-bottom: 0;
      }
    }

    // 段落样式
    p {
      margin: 0 0 1em 0;
      line-height: 1.7;
    }

    // 标题样式
    h1, h2, h3, h4, h5, h6 {
      color: var(--pad-text-color-100);
      font-weight: 600;
      margin: 1.5em 0 0.75em 0;
      line-height: 1.3;

      &:first-child {
        margin-top: 0;
      }
    }

    h1 {
      font-size: 1.75em;
      border-bottom: 2px solid var(--pad-border-color-300);
      padding-bottom: 0.5em;
    }

    h2 {
      font-size: 1.5em;
      border-bottom: 1px solid var(--pad-border-color-200);
      padding-bottom: 0.4em;
    }

    h3 {
      font-size: 1.25em;
    }

    // 列表样式
    ul, ol {
      padding-left: 1.5em;
      margin: 1em 0;
    }

    li {
      margin-bottom: 0.5em;
      position: relative;

      &::marker {
        color: var(--pad-primary-color);
      }
    }

    ul li::before {
      content: '•';
      color: var(--pad-primary-color);
      font-weight: bold;
      display: inline-block;
      width: 1em;
      margin-left: -1em;
    }

    // 链接样式
    a {
      color: var(--pad-primary-color);
      text-decoration: none;
      border-bottom: 1px solid transparent;
      transition: all var(--pad-transition-fast);

      &:hover {
        border-bottom-color: var(--pad-primary-color);
        color: var(--pad-primary-dark);
      }
    }

    // 代码块和内联代码
    pre {
      background-color: var(--pad-bg-color-200);
      border: 1px solid var(--pad-border-color-100);
      border-radius: var(--pad-radius-md);
      padding: 16px;
      overflow-x: auto;
      margin: 1em 0;
      font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
      font-size: 0.9em;
      line-height: 1.5;
    }

    code {
      background-color: var(--pad-bg-color-200);
      color: var(--pad-text-color-300);
      padding: 2px 6px;
      border-radius: var(--pad-radius-sm);
      font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
      font-size: 0.9em;
      border: 1px solid var(--pad-border-color-100);
    }

    // 引用样式
    blockquote {
      border-left: 4px solid var(--pad-primary-color);
      background-color: var(--pad-bg-color-200);
      padding: 12px 20px;
      margin: 1em 0;
      border-radius: 0 var(--pad-radius-md) var(--pad-radius-md) 0;
      font-style: italic;
      color: var(--pad-text-color-200);

      p {
        margin: 0;
      }
    }

    // 水平线
    hr {
      border: none;
      height: 1px;
      background-color: var(--pad-border-color-100);
      margin: 2em 0;
    }

    // 媒体元素
    ::v-deep(.media-item) {
      max-width: clamp(120px, calc(100% - 20px), 600px);
      display: block;
      margin: 0.5em auto;
      box-shadow: var(--pad-shadow-md);
      border-radius: var(--pad-radius-lg);
      border: 1px solid var(--pad-border-color-200);
      transition: all var(--pad-transition-base);
      background-color: var(--pad-bg-color-200);

      &:hover {
        box-shadow: var(--pad-shadow-lg);
        border-color: var(--pad-border-color-300);
        transform: translateY(-1px);
      }

      // 图片样式
      &[data-filename$=".jpg"],
      &[data-filename$=".jpeg"],
      &[data-filename$=".png"],
      &[data-filename$=".gif"],
      &[data-filename$=".webp"] {
        cursor: zoom-in;

        &:hover {
          transform: scale(1.01);
        }
      }

      // 视频样式
      &[data-filename$=".mp4"],
      &[data-filename$=".webm"],
      &[data-filename$=".ogg"] {
        aspect-ratio: 16/9;
        background-color: var(--pad-bg-color-300);
      }

      // 音频样式
      &[data-filename$=".mp3"],
      &[data-filename$=".wav"],
      &[data-filename$=".ogg"] {
        min-height: 80px;
        padding: 16px;
        background-color: var(--pad-bg-color-200);
      }
    }

    // 表格样式
    table {
      width: 100%;
      border-collapse: collapse;
      margin: 1.5em 0;
      border: 1px solid var(--pad-border-color-100);
      border-radius: var(--pad-radius-md);
      overflow: hidden;

      th, td {
        padding: 12px 16px;
        border-bottom: 1px solid var(--pad-border-color-100);
        text-align: left;
      }

      th {
        background-color: var(--pad-bg-color-200);
        font-weight: 600;
        color: var(--pad-text-color-100);
      }

      tr {
        transition: background-color var(--pad-transition-fast);

        &:hover {
          background-color: var(--pad-bg-color-200);
        }

        &:last-child td {
          border-bottom: none;
        }
      }
    }
  }

  .edit {
    width: calc(100% - 1px);
    height: calc(100% - 1px);
    max-width: 100%;
    max-height: 100%;
    overflow-y: auto;
    box-sizing: border-box;
    font-family: inherit;
    font-size: inherit;
    line-height: 1.6;
    resize: none;
    border: 1px solid var(--pad-border-color-200);
    outline: none;
    background-color: var(--pad-bg-color-100);
    padding: 12px;
    color: var(--pad-text-color-100);
    transition: all var(--pad-transition-base);

    &:focus {
      border-color: var(--pad-primary-color);
      box-shadow: 0 0 0 3px var(--pad-primary-color-light), var(--pad-shadow-sm);
    }

    &::placeholder {
      color: var(--pad-text-color-400);
      font-style: italic;
    }

    // 选中的文本
    &::selection {
      background-color: var(--pad-success-color);
      color: var(--pad-text-color-100);
    }
  }
}

// 移动端适配
@media (max-width: 512px) {
  .rich-text-editor {
    font-size: 15px;

    .view {
      padding: 8px;

      &.has-media {
        padding: 16px;
      }

      h1 {
        font-size: 1.5em;
      }

      h2 {
        font-size: 1.3em;
      }

      h3 {
        font-size: 1.15em;
      }

      ::v-deep(.media-item) {
        max-width: calc(100% - 16px);
        border-radius: var(--pad-radius-md);
      }
    }

    .edit {
      /* 在移动设备上默认添加底部边距 避免软键盘弹起高度问题 */
      padding: 8px 8px 250px;
      font-size: 15px;
    }
  }
}

// 平板适配
@media (min-width: 513px) and (max-width: 1024px) {
  .rich-text-editor {
    .view {
      padding: 18px;

      &.has-media {
        padding: 22px;
      }

      ::v-deep(.media-item) {
        max-width: clamp(150px, calc(100% - 24px), 500px);
      }
    }

    .edit {
      padding: 18px;
    }
  }
}
</style>
