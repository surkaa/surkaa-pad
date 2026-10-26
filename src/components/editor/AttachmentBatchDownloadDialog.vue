<script setup lang="ts">
import {computed, ref, watch} from 'vue';
import type {AttachmentMeta} from '../../bindings';
import {formatBytes} from '../../utils/format';
import DetailsHelpButton from '../DetailsHelpButton.vue';

const props = defineProps<{modelValue: boolean; attachments: readonly AttachmentMeta[]; initialAttachmentId: string}>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'download', attachmentIds: string[]): void;
}>();
const visible = computed({get: () => props.modelValue, set: value => emit('update:modelValue', value)});
const query = ref('');
const selected = ref<string[]>([]);
const available = computed(() => [...new Map(props.attachments.map(item => [item.id, item])).values()]);
const filtered = computed(() => {
  const term = (query.value ?? '').trim().toLocaleLowerCase();
  return available.value.filter(item => item.filename.toLocaleLowerCase().includes(term));
});
const selectedSet = computed(() => new Set(selected.value));
const allFilteredSelected = computed(() => filtered.value.length > 0 && filtered.value.every(item => selectedSet.value.has(item.id)));
const selectedBytes = computed(() => available.value.reduce((sum, item) => sum + (selectedSet.value.has(item.id) ? item.size : 0), 0));
watch(() => props.modelValue, value => {
  if (!value) return;
  query.value = '';
  selected.value = available.value.some(item => item.id === props.initialAttachmentId) ? [props.initialAttachmentId] : [];
}, {immediate: true});
watch(available, items => {
  const ids = new Set(items.map(item => item.id));
  selected.value = selected.value.filter(id => ids.has(id));
});
function toggle(id: string) {
  selected.value = selectedSet.value.has(id) ? selected.value.filter(item => item !== id) : [...selected.value, id];
}
function toggleAll() {
  const ids = new Set(filtered.value.map(item => item.id));
  selected.value = allFilteredSelected.value ? selected.value.filter(id => !ids.has(id)) : [...new Set([...selected.value, ...ids])];
}
function download() {
  const ids = available.value.filter(item => selectedSet.value.has(item.id)).map(item => item.id);
  if (!ids.length) return;
  emit('download', ids);
  visible.value = false;
}
</script>

<template>
  <q-dialog v-model="visible" no-refocus>
    <q-card class="attachment-batch-dialog">
      <q-card-section class="row items-center no-wrap q-gutter-sm">
        <div class="text-h6 col">批量下载附件</div>
        <DetailsHelpButton title="批量下载说明" in-dialog>
          <p>选择一次文件夹，逐个保存解密后的原文件，不修改日记或云端附件。同名文件会自动编号，不覆盖已有文件。</p>
          <p>取消会停止剩余下载，已完成的文件保留；当前未完成文件会尝试清理。Android 请选择系统允许授权的文件夹。</p>
        </DetailsHelpButton>
        <q-btn flat round dense icon="close" aria-label="关闭批量下载" v-close-popup/>
      </q-card-section>
      <q-card-section class="q-pt-none batch-filter">
        <q-input v-model="query" outlined dense clearable :dark="$q.dark.isActive" label="搜索附件文件名"
                 @clear="query = ''"/>
        <div class="row items-center justify-between q-mt-sm q-gutter-sm">
          <q-btn flat dense color="primary" :label="allFilteredSelected ? '取消选择当前结果' : '全选当前结果'"
                 :disable="!filtered.length" @click="toggleAll"/>
          <span class="batch-description">{{ filtered.length }} 个附件</span>
        </div>
      </q-card-section>
      <q-separator/>
      <q-virtual-scroll v-if="filtered.length" class="batch-files" :items="filtered" :virtual-scroll-item-size="64">
        <template #default="{item}">
          <q-item :key="item.id" tag="label" class="batch-file" :data-attachment-id="item.id">
            <q-item-section avatar>
              <q-checkbox :model-value="selectedSet.has(item.id)" :dark="$q.dark.isActive"
                          :aria-label="`选择 ${item.filename}`" @update:model-value="toggle(item.id)"/>
            </q-item-section>
            <q-item-section class="batch-file-name">
              <q-item-label lines="1" :title="item.filename">{{ item.filename }}</q-item-label>
              <q-item-label caption class="batch-description">{{ formatBytes(item.size) }}</q-item-label>
            </q-item-section>
          </q-item>
        </template>
      </q-virtual-scroll>
      <div v-else class="batch-files batch-empty">没有匹配的附件</div>
      <q-separator/>
      <q-card-actions class="batch-actions" align="right">
        <span class="batch-description col">已选 {{ selected.length }} 个 · {{ formatBytes(selectedBytes) }}</span>
        <q-btn flat color="primary" label="取消" v-close-popup/>
        <q-btn unelevated color="primary" label="选择文件夹并下载" :disable="!selected.length" @click="download"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<style scoped>
.attachment-batch-dialog {
  width: min(640px, calc(100vw - 32px));
  max-height: calc(100dvh - 48px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
}
.attachment-batch-dialog > :not(.batch-files) { flex-shrink: 0; }
.batch-files { flex: 1 1 auto; min-height: 0; height: 55dvh; max-height: 420px; overflow-y: auto; }
.batch-file { min-height: 64px; }
.batch-file-name { min-width: 0; }
.batch-description { color: var(--pad-text-color-400); font-size: 12px; }
.batch-empty { display: grid; place-items: center; color: var(--pad-text-color-300); }
.batch-actions { flex-wrap: wrap; gap: 6px; }
@media (max-width: 420px) { .batch-actions > span { flex-basis: 100%; padding: 0 8px; } }
</style>
