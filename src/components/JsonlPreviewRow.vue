<template>
  <article class="jsonl-row" :data-line="line.lineNumber">
    <q-resize-observer @resize="emit('resize')"/>
    <div class="jsonl-row-header">
      <q-btn flat dense no-caps color="primary" :icon="expanded ? 'expand_less' : 'chevron_right'"
        :label="`第 ${line.lineNumber} 行`" :aria-expanded="expanded" @click="expanded = !expanded"/>
      <span v-if="parsed.error" class="jsonl-row-warning" :title="parsed.error">{{ parsed.error }}</span>
      <q-space/>
      <q-btn flat round dense icon="content_copy" color="primary" :aria-label="`复制第 ${line.lineNumber} 行`" @click="copyLine"/>
    </div>
    <JsonTreeViewer v-if="expanded && parsed.treeSafe" :source="parsed.value" :expand-json-strings-by-default="false" expand-to-content/>
    <pre v-else class="jsonl-row-text" :class="{'jsonl-row-text--expanded': expanded}"
      @dblclick="expanded = !expanded">{{ expanded ? line.text : previewText }}</pre>
  </article>
</template>

<script setup lang="ts">
import {computed, ref} from 'vue';
import {useQuasar} from 'quasar';
import {parseJsonlLine, type JsonlLine} from '../utils/jsonlPreview';
import {copyTextToClipboard} from '../utils/clipboard';
import {formatError} from '../utils/formatError';
import JsonTreeViewer from './JsonTreeViewer.vue';

const props = defineProps<{line: JsonlLine}>();
const emit = defineEmits<{(event: 'resize'): void}>();
const $q = useQuasar();
const expanded = ref(false);
const parsed = computed(() => parseJsonlLine(props.line));
const previewText = computed(() => props.line.text.length > 1000 ? `${props.line.text.slice(0, 1000)}…` : props.line.text || '（空行）');

async function copyLine() {
  try {
    await copyTextToClipboard(props.line.text);
    $q.notify({type: 'positive', message: '该行原文已复制'});
  } catch (error) { $q.notify({type: 'negative', message: `复制失败：${formatError(error)}`}); }
}
</script>

<style scoped lang="scss">
.jsonl-row {
  position: relative;
  min-height: 104px;
  border-bottom: 1px solid var(--pad-border-color-100);
  background: var(--pad-bg-color-100);
}
.jsonl-row-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  color: var(--pad-text-color-100);
}
.jsonl-row-warning {
  min-width: 0;
  overflow: hidden;
  color: var(--pad-warning-color);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.jsonl-row-text {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  margin: 0;
  padding: 0 16px 12px;
  overflow: hidden;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font: 13px/1.6 ui-monospace, SFMono-Regular, Consolas, monospace;
  color: var(--pad-text-color-200);
  user-select: text;
  -webkit-user-select: text;
}
.jsonl-row-text--expanded {
  display: block;
  max-height: 420px;
  overflow: auto;
}
@media (max-width: 600px) {
  .jsonl-row-header { padding-inline: 6px; }
  .jsonl-row-text { padding-inline: 12px; }
}
</style>
