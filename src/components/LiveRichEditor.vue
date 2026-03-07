<script setup lang="ts">
import {nextTick, onMounted, ref, watch} from "vue";
import {DiarySummary} from "../bindings.ts";
import {ExtensionContext, EXTENSIONS} from "./editor/extension.ts";
import {useRouter} from "vue-router";
import {useContextMenu} from "./editor/useContextMenu.ts";
import {useScroll, useStorage} from "@vueuse/core";

const router = useRouter();
const {modelValue, diarySummary, attachmentMap} = defineProps<{
  modelValue: string;
  diarySummary?: DiarySummary;
  attachmentMap: Record<string, string>;
}>();
const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'toggleAttachmentEncryption', filename: string, encrypted: boolean): Promise<void>;
  (e: 'rotateAttachment', filename: string, rotation: number): void;
}>();

const editor = ref<HTMLDivElement>();

const storageY = useStorage(`scroll-y-${diarySummary?.id}`, 0, sessionStorage);
const {y} = useScroll(editor, {
  behavior: 'smooth',
  onScroll() {
    storageY.value = y.value;
  }
});

const extensionCtx: ExtensionContext = {
  getDiaryId: () => diarySummary?.id || '',
  getAttachment: (filename) => {
    if (!diarySummary) return null;
    // 纯函数查询，不要在这里做任何 emit 或副作用
    return diarySummary.attachments.find(att => att.filename === filename) || null;
  },
  getAttachmentUrl: (filename) => {
    if (!diarySummary) return null;
    const att = diarySummary.attachments.find(att => att.filename === filename);
    if (!att || !attachmentMap[att.filename]) return null;
    return attachmentMap[att.filename];
  },
  gotoPreview: (src, rotation) => router.push({
    name: 'PreviewMedia',
    params: {src, rotation}
  }),
  emit: {
    rotateAttachment(filename: string, rotation: number) {
      emit('rotateAttachment', filename, rotation);
    }
  }
}

const {handleEditorContextMenu} = useContextMenu(
    extensionCtx,
    handleInput,
    (filename, encrypted) => emit('toggleAttachmentEncryption', filename, encrypted)
);

function parseSourceToHtml(source: string): string {
  let result = source;
  for (const ext of EXTENSIONS) {
    if (ext.toHtml) result = ext.toHtml(result, extensionCtx);
  }
  return result;
}

function parseHtmlToSource(html: string): string {
  // 利用原生 DOMParser 建立沙箱，自动修复未闭合标签 抵御脏 HTML
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');

  // 深度优先遍历，安全拦截并替换媒体节点为 Source 文本
  const walkAndReplace = (node: Node) => {
    // 转换成数组防止操作子节点时引发迭代器异常
    const children = Array.from(node.childNodes);
    for (const child of children) {
      walkAndReplace(child); // 向下递归

      const ext = EXTENSIONS.find(e => e.match && e.match(child));
      if (ext && ext.serialize) {
        // 调用插件生成 [[IMG:xxx]] 等标记
        const sourceText = ext.serialize(child as HTMLElement);
        // 原地替换：将复杂的 <img>/<audio> 节点替换为纯文本节点
        node.replaceChild(document.createTextNode(sourceText), child);
      }
    }
  };

  walkAndReplace(doc.body);

  // 提取脱水后的 HTML 此时已剥离所有多媒体元素
  let result = doc.body.innerHTML;

  // 让 BaseExtension 处理剩下的基础标签 如 br, div 转换为 \n
  for (const ext of EXTENSIONS) {
    if (ext.toSource && !ext.serialize) {
      result = ext.toSource(result);
    }
  }

  return result;
}

function handleInput() {
  if (!editor.value) return;
  emit('update:modelValue', parseHtmlToSource(editor.value.innerHTML));
}

function tryUpdateHtml(editorElement: HTMLDivElement, newVal: string) {
  // 获取当前编辑器内容反解析出来的 Source
  const currentSource = parseHtmlToSource(editorElement.innerHTML);

  // 将当前 Source 与外部传入的新 newVal 作对比
  if (currentSource !== newVal) {
    // 只有真正不一致时，才重写 innerHTML
    editorElement.innerHTML = parseSourceToHtml(newVal);
  }
}

// 处理点击 分发 onClick
function handleEditorClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (!editor.value) return;

  let current: HTMLElement | null = target;
  while (current && current !== editor.value) {
    const handler = EXTENSIONS.find(ext => ext.match && ext.match(current!));
    if (handler && handler.onClick) {
      handler.onClick(e, current, extensionCtx);
      return;
    }
    current = current.parentElement;
  }
}

// 暴露editor给父组件
defineExpose({
  editor,
  updateSrc(id: string, newUrl: string) {
    if (!editor.value) return false;
    const el = editor.value.querySelector(`img[data-id="${id}"]`);
    if (!el) return false;
    if (el instanceof HTMLMediaElement) {
      el.src = newUrl;
      el.load();
    } else if (el instanceof HTMLImageElement) {
      el.src = newUrl;
    } else {
      console.warn('无法更新附件URL，未知元素类型:', el);
      return false;
    }
    return true;
  }
});

onMounted(async () => {
  if (!editor.value) {
    await nextTick(); // 确保 DOM 已经更新
  }
  if (!editor.value) {
    console.log('Editor element not found after nextTick');
    return;
  }
  tryUpdateHtml(editor.value, modelValue);
  await nextTick();
  if (storageY.value > 0) {
    editor.value.scrollTop = storageY.value;
  }
});

watch(() => modelValue, (newVal) => {
  if (!editor.value) return;
  tryUpdateHtml(editor.value, newVal);
});
</script>

<template>
  <div
      v-bind="$attrs"
      class="live-rich-editor"
      ref="editor"
      contenteditable="true"
      @input="handleInput"
      @click="handleEditorClick"
      @contextmenu="handleEditorContextMenu"
  ></div>
</template>

<style scoped lang="scss">
.live-rich-editor {
  width: 100%;
  box-sizing: border-box;
  outline: none;
  text-align: left;
  flex: 1;
  overflow-y: auto;
  height: 0;
}
</style>

<style lang="scss">
.live-rich-editor {
  img[data-id] {
    cursor: pointer;
    min-height: 50px;
    transition: width 0.3s ease;
    width: auto;
  }

  img[data-id]:hover {
    box-shadow: 0 0 0 3px rgba(64, 158, 255, 0.5);
  }

  img[data-size="small"] {
    width: 32% !important;
    aspect-ratio: 1 / 1;
    object-fit: cover;
    display: inline-block;
  }

  audio[data-id] {
    width: 90%;
    margin: 10px auto;
  }

  video[data-id] {
    border-radius: 8px;
    margin: 10px 0;
    background: #000;
  }

  img, video, audio {
    padding: 5px;
    // 给右侧编辑区留足点击空间用来删除媒体
    max-width: calc(100% - 10px);
    -webkit-touch-callout: none; /* 禁用 iOS/Android 默认长按菜单 */
    user-select: none; /* 禁用文本/元素选区 */
    -webkit-user-select: none;
    -webkit-user-drag: none; /* 禁用原生拖拽 */
  }

  // 通用文件卡片样式
  // <div class="file-title"><span class="file-icon">📎</span><span class="file-name">${filename}</span></div><span class="file-size">${filesizeText}</span>
  .editor-file-attachment {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    margin: 8px 5px;
    background-color: var(--pad-bg-color);
    border: 1px solid var(--pad-border-color);
    border-radius: 6px;
    cursor: pointer;
    -webkit-user-select: none;
    transition: all 0.2s ease;
    width: 100%;

    &:hover {
      background-color: var(--pad-bg-color-300);
      border-color: var(--pad-border-color-300);
      color: var(--pad-text-color-300);
    }

    .file-title {
      display: flex;
      align-items: center;

      .file-icon {
        font-size: 1.2em;
        margin-right: 8px;
      }

      .file-name {
        font-size: 14px;
        color: var(--pad-text-color);
        word-break: break-all;
        overflow: hidden;
        text-overflow: ellipsis;
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
      }
    }
  }
}
</style>
