<script setup lang="ts">
import {ref, watch} from "vue";
import {BaseExtension} from "./editor/baseExtension.ts";
import {ImageExtension} from "./editor/imageExtension.ts";
import {DiarySummary} from "../bindings.ts";
import {ExtensionContext} from "./editor/extension.ts";

const {modelValue, diarySummary} = defineProps<{
  modelValue: string;
  diarySummary?: DiarySummary;
}>();
const emit = defineEmits(['update:modelValue']);

const editor = ref<HTMLDivElement>();
const extensions = [
  ImageExtension,
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

// 声明不自动继承属性
defineOptions({
  inheritAttrs: false
});

// 暴露editor给父组件
defineExpose({
  editor
});

watch([() => modelValue, editor], ([newVal, _]) => {
  if (!editor.value) return;
  // 获取当前编辑器内容反解析出来的 Source
  const currentSource = parseHtmlToSource(editor.value.innerHTML);

  // 将当前 Source 与外部传入的新 newVal 作对比
  if (currentSource !== newVal) {
    // 只有真正不一致时，才重写 innerHTML
    editor.value.innerHTML = parseSourceToHtml(newVal);
  }
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
  ></div>
</template>

<style scoped lang="scss">
.live-rich-editor {
  width: 100%;
  height: 100%;
  padding: 16px;
  box-sizing: border-box;
  outline: none;
  text-align: left;
}
</style>
