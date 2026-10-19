<template>
  <section class="jsonl-preview">
    <q-resize-observer @resize="refreshRows"/>
    <div class="jsonl-toolbar">JSONL · 已读取 {{ lines.length }} 行<span>每次最多加载 10 行，点击行号展开</span></div>
    <div ref="scrollTarget" class="jsonl-scroll scroll" @scroll.passive="onScroll">
      <q-virtual-scroll v-if="lines.length" ref="virtualList" :items="lines" :scroll-target="scrollTarget || undefined"
        :virtual-scroll-item-size="104" :virtual-scroll-slice-size="15" v-slot="{item}: {item: JsonlLine}">
        <JsonlPreviewRow :key="item.lineNumber" :line="item" @resize="refreshRows"/>
      </q-virtual-scroll>
      <div v-else-if="finished" class="jsonl-empty">文件为空</div>
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
import {nextTick, ref, watch} from 'vue';
import type {QVirtualScroll} from 'quasar';
import {useJsonlPreview} from '../composables/useJsonlPreview';
import type {JsonlLine} from '../utils/jsonlPreview';
import JsonlPreviewRow from './JsonlPreviewRow.vue';

const props = defineProps<{url: string}>();
const scrollTarget = ref<HTMLElement>();
const virtualList = ref<QVirtualScroll>();
const {lines, loading, finished, error, loadMore} = useJsonlPreview(() => props.url);
let refreshQueued = false;

async function refreshRows() {
  if (refreshQueued) return;
  refreshQueued = true;
  await nextTick();
  refreshQueued = false;
  // JSON 树展开、内部节点切换和全屏变化后重新测量，避免虚拟列表沿用旧高度。
  virtualList.value?.refresh();
  fillViewport();
}

function onScroll() {
  const element = scrollTarget.value;
  if (!element || error.value || element.clientHeight <= 0) return;
  if (element.scrollHeight - element.scrollTop - element.clientHeight < 120) void loadMore();
}
watch(loading, async busy => {
  if (busy || error.value || finished.value) return;
  await nextTick();
  fillViewport();
});

function fillViewport() {
  if (loading.value || error.value || finished.value) return;
  const element = scrollTarget.value;
  // 全屏或超大窗口下十行可能不足一屏；继续补一页，避免无滚动空间时无法触发加载。
  if (element && element.clientHeight > 0 && element.scrollHeight <= element.clientHeight) void loadMore();
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
