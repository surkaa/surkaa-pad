<script setup lang="ts">
import {ref, watch} from "vue";
import {BaseExtension} from "./editor/baseExtension.ts";

const {modelValue} = defineProps<{
  modelValue: string;
}>();
const emit = defineEmits(['update:modelValue']);

const editor = ref<HTMLDivElement>();
const extensions = [
  BaseExtension
];
const styles = extensions.map(ext => ext.style || '').join("\n");

function parseSourceToHtml(source: string): string {
  let result = source;
  for (const ext of extensions) {
    if (ext.toHtml) result = ext.toHtml(result);
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

watch(() => modelValue, (newVal) => {
  if (!editor.value) return;
  const newHtml = parseSourceToHtml(newVal);
  if (editor.value.innerHTML !== newHtml) {
    editor.value.innerHTML = newHtml;
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
