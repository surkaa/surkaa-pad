<script setup lang="ts">
import {nextTick, onMounted, ref, watch} from "vue";
import {DiarySummary} from "../bindings.ts";
import {ExtensionContext, EXTENSIONS} from "./editor/extension.ts";
import {useRouter} from "vue-router";

const router = useRouter();
const {modelValue, diarySummary} = defineProps<{
  modelValue: string;
  diarySummary?: DiarySummary;
}>();
const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'attachmentNoFount', filename: string, mark: string): void;
}>();

const editor = ref<HTMLDivElement>();

const extensionCtx: ExtensionContext = {
  getDiaryId: () => diarySummary?.id || '',
  getAttachment: (filename, mark) => {
    if (!diarySummary) return null;
    const attachment = diarySummary.attachments.find(att => att.filename === filename);
    if (!attachment) {
      emit('attachmentNoFount', filename, mark);
      return null;
    }
    return attachment;
  },
  gotoPreview: (type, diaryId, filename) => router.push({
    name: 'PreviewMedia',
    params: {type, diaryId, filename}
  }),
}

function parseSourceToHtml(source: string): string {
  let result = source;
  for (const ext of EXTENSIONS) {
    if (ext.toHtml) result = ext.toHtml(result, extensionCtx);
  }
  return result;
}

function parseHtmlToSource(html: string): string {
  let result = html;
  for (const ext of EXTENSIONS) {
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
  const handler = EXTENSIONS.find(ext => ext.match && ext.match(target));
  if (handler && handler.onClick) {
    handler.onClick(e, target, extensionCtx);
  }
};

// 暴露editor给父组件
defineExpose({
  editor
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
})

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

<style lang="scss">
.live-rich-editor {
  img[data-id] {
    padding: 5px;
    cursor: pointer;
    min-height: 50px;
    transition: width 0.3s ease;
    width: auto;
    max-width: 100%;
  }

  img[data-id]:hover {
    box-shadow: 0 0 0 3px rgba(64, 158, 255, 0.5);
  }

  img[data-size="small"] {
    width: 33% !important;
    display: inline-block;
  }

  audio[data-id] {
    width: 100%;
    margin: 10px 0;
  }

  video[data-id] {
    max-width: 100%;
    border-radius: 8px;
    margin: 10px 0;
    background: #000;
  }
}
</style>
