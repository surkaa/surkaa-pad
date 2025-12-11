<script setup lang="ts">
// 要实现的功能：
// 渲染内容文本（包括媒体文件）
import {DiaryManifest} from "../types";
import {computed, onMounted, onUnmounted, ref, watch} from "vue";
import {convertFilename2URL} from "../utils/convertFilename2URL.ts";
import {useAppStore} from "../stores/app.ts";

const appStore = useAppStore();
const model = defineModel<string>({default: ''});
const {
    diary,
    mode
} = defineProps<{
  diary: DiaryManifest;
  mode: 'edit' | 'view'
}>();
const cancelFns = ref<Function[]>([]);

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
      return el.outerHTML;
    }
    // 请求将文件名转换为URL
    const cFn = convertFilename2URL(diary.id, a.nonce, eid, a.mimetype, fn, (url: string) => {
      // 设置媒体元素的src属性
      const element = document.getElementById(eid) as HTMLMediaElement;
      if (element) {
        // 从cancelFns中移除这个取消函数
        const index = cancelFns.value.indexOf(cFn);
        if (index !== -1) {
          cancelFns.value.splice(index, 1);
        }
        element.src = url;
      }
    });
    cancelFns.value.push(cFn);

    if (tag === 'IMG') {
      return `<img class="media-item" id="${eid}" alt="${fn}" /><br>`;
    } else if (tag === 'VID') {
      return `<video class="media-item" id="${eid}" controls></video><br>`;
    } else if (tag === 'AUD') {
      return `<audio class="media-item" id="${eid}" controls></audio><br>`;
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
  })
});
</script>

<template>
  <div class="rich-text-editor">
    <textarea class="edit" v-show="isEditing" v-model="model"/>
    <div class="view" v-show="!isEditing" v-html="innerHTML"/>
  </div>
</template>

<style scoped lang="scss">
.rich-text-editor {
  width: 100%;
  height: 100%;
  text-align: left;

  .view {
    white-space: pre-wrap;
    word-break: break-word;

    ::v-deep(.media-item) {
      max-width: 100%;
    }

    ::v-deep(mark) {
      background-color: yellow;
    }
  }

  .edit {
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    font-family: inherit;
    font-size: inherit;
    line-height: inherit;
    resize: none;
  }
}
</style>