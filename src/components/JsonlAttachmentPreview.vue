<template>
  <section class="jsonl-preview">
    <q-resize-observer @resize="refreshRows"/>
    <div class="jsonl-toolbar">JSONL · 已读取 {{ lines.length }} 行<span>每次最多加载 10 行，点击行号展开</span></div>
    <div ref="scrollTarget" class="jsonl-scroll scroll" :style="{'--jsonl-row-min-height': `${JSONL_ROW_MIN_HEIGHT}px`}">
      <q-infinite-scroll v-if="scrollTarget" ref="infiniteScroll" :scroll-target="scrollTarget" :offset="LOAD_OFFSET"
                         :disable="!viewportHeight || !lines.length || finished || Boolean(error)" @load="onLoad">
        <!-- 重测切片时保留已加载行的最小总高度，避免暂时缩短的占位触发假触底。 -->
        <q-virtual-scroll v-if="lines.length" ref="virtualList" :items="lines" :scroll-target="scrollTarget"
          :style="{minHeight: `${lines.length * JSONL_ROW_MIN_HEIGHT}px`}"
          :virtual-scroll-item-size="JSONL_ROW_MIN_HEIGHT" :virtual-scroll-slice-size="15" v-slot="{item}: {item: JsonlLine}">
          <JsonlPreviewRow :key="item.lineNumber" :line="item" @resize="refreshRows"/>
        </q-virtual-scroll>
      </q-infinite-scroll>
      <div v-if="finished && !lines.length" class="jsonl-empty">文件为空</div>
    </div>
    <div class="jsonl-status" role="status">
      <template v-if="loading"><q-spinner color="primary" size="20px"/>正在读取后续行…</template>
      <template v-else-if="error"><span class="jsonl-error">{{ error }}</span><q-btn flat dense label="重试" color="primary" @click="loadMore"/></template>
      <span v-else-if="finished && lines.length">已到文件末尾</span>
      <q-btn v-else-if="!finished" flat dense label="加载后续 10 行" color="primary" @click="loadMore"/>
    </div>
  </section>
</template>

<script setup lang="ts">
import {nextTick, ref} from 'vue';
import type {QInfiniteScroll, QVirtualScroll} from 'quasar';
import {useJsonlPreview} from '../composables/useJsonlPreview';
import {JSONL_ROW_MIN_HEIGHT, type JsonlLine} from '../utils/jsonlPreview';
import JsonlPreviewRow from './JsonlPreviewRow.vue';

const props = defineProps<{url: string}>();
const LOAD_OFFSET = 120;
const scrollTarget = ref<HTMLElement>();
const virtualList = ref<QVirtualScroll>();
const infiniteScroll = ref<QInfiniteScroll>();
const viewportHeight = ref(0);
const {lines, loading, finished, error, loadMore} = useJsonlPreview(() => props.url);
let refreshQueued = false;

async function refreshRows() {
  if (refreshQueued) return;
  refreshQueued = true;
  await nextTick();
  refreshQueued = false;
  viewportHeight.value = scrollTarget.value?.clientHeight ?? 0;
  // 尺寸变化只重测虚拟列表，不直接发请求。加载统一交给 InfiniteScroll 调度。
  virtualList.value?.refresh();
  infiniteScroll.value?.poll();
}

async function onLoad(_index: number, done: (stop?: boolean) => void) {
  const element = scrollTarget.value;
  // InfiniteScroll 恢复时可能早于虚拟列表 DOM 更新，不能用上一帧的短高度判断触底。
  const minimumHeight = lines.value.length * JSONL_ROW_MIN_HEIGHT;
  if (!element || element.clientHeight <= 0
    || element.scrollTop + element.clientHeight + LOAD_OFFSET < Math.max(element.scrollHeight, minimumHeight)) {
    done(false); return;
  }
  // 与 DiaryList 一样使用单次加载 / done 握手，避免重复滚动事件并发请求。
  if (loading.value) { done(false); return; }
  await loadMore();
  done(finished.value || Boolean(error.value));
}
</script>

<style scoped lang="scss">
.jsonl-preview {
  position: relative;
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-200);
}
.jsonl-toolbar {
  display: flex;
  flex: none;
  flex-wrap: wrap;
  gap: 6px 16px;
  padding: 10px 16px;
  font-size: 13px;
  background: var(--pad-bg-color-200);
}
.jsonl-toolbar span { color: var(--pad-text-color-400); }
.jsonl-scroll { flex: 1; min-height: 0; overflow: auto; }
.jsonl-status {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 42px;
  padding: 6px 12px;
  color: var(--pad-text-color-400);
  font-size: 13px;
  border-top: 1px solid var(--pad-border-color-100);
}
.jsonl-error { min-width: 0; overflow-wrap: anywhere; color: var(--q-negative); }
.jsonl-empty { padding: 36px; text-align: center; color: var(--pad-text-color-400); }
</style>
