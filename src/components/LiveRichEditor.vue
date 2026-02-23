<script setup lang="ts">
import {ref, watch} from "vue";
import {BaseExtension} from "./editor/baseExtension.ts";
import {ImageExtension} from "./editor/imageExtension.ts";
import {DiarySummary} from "../bindings.ts";
import {ExtensionContext} from "./editor/extension.ts";
import {AudioExtension} from "./editor/audioExtension.ts";
import {VideoExtension} from "./editor/videoExtension.ts";

const {modelValue, diarySummary} = defineProps<{
  modelValue: string;
  diarySummary?: DiarySummary;
}>();
const emit = defineEmits(['update:modelValue']);

const editor = ref<HTMLDivElement>();
let observer: MutationObserver | null = null;

const extensions = [
  ImageExtension,
  AudioExtension,
  VideoExtension,
  BaseExtension
];
const styles = extensions.map(ext => ext.style || '').join("\n");

const extensionCtx: ExtensionContext = {
  getDiaryId: () => diarySummary?.id || '',
  getAttachment: (filename) => {
    if (!diarySummary) return null;
    const attachment = diarySummary.attachments.find(att => att.filename === filename);
    return attachment || null;
  },
}

function parseSourceToHtml(source: string): string {
  let result = source;
  for (const ext of extensions) {
    if (ext.toHtml) result = ext.toHtml(result, extensionCtx);
  }
  return result;
}

function parseHtmlToSource(html: string): string {
  let result = html;
  for (const ext of extensions) {
    if (ext.toSource) result = ext.toSource(result);
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
const handleEditorClick = (e: MouseEvent) => {
  const target = e.target as HTMLElement;

  // 遍历插件，寻找谁负责这个节点
  const handler = extensions.find(ext => ext.match && ext.match(target));
  if (handler && handler.onClick) {
    handler.onClick(e, target, extensionCtx);
  }
};

// 声明不自动继承属性
defineOptions({
  inheritAttrs: false
});

// 暴露editor给父组件
defineExpose({
  editor
});

watch(editor, (newEditor) => {
  if (!newEditor) return;
  tryUpdateHtml(newEditor, modelValue);
  observer = new MutationObserver((mutations) => {
    mutations.forEach((mutation) => {
      // 我们只关心节点被移除的情况
      if (mutation.type === 'childList' && mutation.removedNodes.length > 0) {
        mutation.removedNodes.forEach((node) => {
          const handler = extensions.find(ext => ext.match && ext.match(node));
          if (handler && handler.onDeleted) {
            handler.onDeleted(node, extensionCtx);
          }
        });
      }
    });
  });
  // 监听 childList (子节点变化) 和 subtree (后代节点变化)
  observer.observe(newEditor, {
    childList: true,
    subtree: true,
  });
});

watch(() => modelValue, (newVal) => {
  if (!editor.value) return;
  tryUpdateHtml(editor.value, newVal);
});
</script>

<template>
  <component is="style">{{ styles }}</component>
  <div
      v-bind="$attrs"
      class="live-rich-editor"
      ref="editor"
      contenteditable="true"
      @input="handleInput"
      @click="handleEditorClick"
  ></div>
</template>

<style scoped lang="scss">
.live-rich-editor {
  width: 100%;
  box-sizing: border-box;
  outline: none;
  text-align: left;
}
</style>
