<script setup lang="ts">
// 要实现的功能：
// 渲染内容文本（包括媒体文件）
import {DiaryManifest} from "../types";
import {computed, onMounted} from "vue";

const model = defineModel<string>({default: ''});
const {diary} = defineProps<{ diary: DiaryManifest }>();

const innerHTML = computed(() => {
  return model.value.replace(/<<(IMG|VID|AUD):(.+?)>>/g, (match, tag, filename) => {
    let elementType;
    if (tag === 'IMG') {
      elementType = 'img';
    } else if (tag === 'VID') {
      elementType = 'video';
    } else if (tag === 'AUD') {
      elementType = 'audio';
    } else {
      console.warn(`Unknown tag: ${tag}`);
      return match; // 未知标签，返回原始文本
    }
    const attachment = diary.attachments.find(att => att.filename === filename);
    const attachmentIndex = diary.attachments.findIndex(a => a.filename === filename);
    if (!attachment) return match; // 如果找不到附件，返回原始文本

    // 生成eid
    const eid = `EID_${diary.id}_${attachmentIndex}`;

    if (elementType === 'img') {
      return `<img class="meida-item" id="${eid}" alt="${filename}" />`;
    } else if (elementType === 'video') {
      return `<video class="meida-item" id="${eid}" controls></video>`;
    } else if (elementType === 'audio') {
      return `<audio class="meida-item" id="${eid}" controls></audio>`;
    }
    return match;
  });
});

onMounted(() => {
  console.log(model.value);
});
</script>

<template>
  <div class="rich-text-editor" contenteditable="true" v-html="innerHTML"/>
</template>

<style scoped lang="scss">
.rich-text-editor {
  width: 100%;
  height: 100%;
  text-align: left;

  // 支持换行
  white-space: pre-wrap;
  word-break: break-word;
}
</style>