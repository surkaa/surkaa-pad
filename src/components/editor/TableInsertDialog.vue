<script setup lang="ts">
import {computed, ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import {MAX_TABLE_CELLS, MAX_TABLE_COLUMNS, MAX_TABLE_ROWS, validTableSize} from './tableCommands';

const props = defineProps<{modelValue: boolean}>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'insert', rows: number, columns: number): void;
}>();
const $q = useQuasar();
const rows = ref<number | string | null>(3);
const columns = ref<number | string | null>(3);
const visible = computed({get: () => props.modelValue, set: value => emit('update:modelValue', value)});
const valid = computed(() => validTableSize(Number(rows.value), Number(columns.value)));
// 只有通过现有大小校验才渲染网格，避免输入中的无效数字或超大数创建大量 DOM。
const previewSize = computed(() => valid.value ? {rows: Number(rows.value), columns: Number(columns.value)} : null);
watch(() => props.modelValue, open => { if (open) { rows.value = 3; columns.value = 3; } });
function adjust(field: 'rows' | 'columns', delta: number) {
  const value = field === 'rows' ? rows : columns;
  const max = field === 'rows' ? MAX_TABLE_ROWS : MAX_TABLE_COLUMNS;
  value.value = Math.max(1, Math.min(max, Math.trunc(Number(value.value) || 1) + delta));
}
function confirm() {
  if (!valid.value) return;
  emit('insert', Number(rows.value), Number(columns.value));
}
</script>

<template>
  <q-dialog v-model="visible" no-refocus>
    <q-card class="table-insert-dialog">
      <q-card-section class="table-dialog-header">
        <div class="text-h6">插入表格</div>
        <div class="table-description">第一行为表头，行数包含表头</div>
      </q-card-section>
      <q-card-section class="q-pt-none table-dialog-body">
        <div v-for="field in (['rows', 'columns'] as const)" :key="field" class="table-dimension">
          <span>{{ field === 'rows' ? '行数' : '列数' }}</span>
          <q-btn flat round dense icon="remove" :aria-label="`减少${field === 'rows' ? '行数' : '列数'}`"
                 :disable="Number(field === 'rows' ? rows : columns) <= 1" @click="adjust(field, -1)"/>
          <q-input :model-value="field === 'rows' ? rows : columns" type="number" outlined dense
                   :dark="$q.dark.isActive" :min="1" :max="field === 'rows' ? MAX_TABLE_ROWS : MAX_TABLE_COLUMNS"
                   :step="1" :aria-label="field === 'rows' ? '行数' : '列数'"
                   @update:model-value="value => field === 'rows' ? rows = value : columns = value"
                   @keyup.enter="confirm"/>
          <q-btn flat round dense icon="add" :aria-label="`增加${field === 'rows' ? '行数' : '列数'}`"
                 :disable="Number(field === 'rows' ? rows : columns) >= (field === 'rows' ? MAX_TABLE_ROWS : MAX_TABLE_COLUMNS)"
                 @click="adjust(field, 1)"/>
        </div>
        <div class="table-description" :class="{'text-negative': !valid}">
          最多 {{ MAX_TABLE_ROWS }} 行、{{ MAX_TABLE_COLUMNS }} 列，总计不超过 {{ MAX_TABLE_CELLS }} 个单元格
        </div>
        <div class="table-preview-heading">
          <span>预览</span>
          <span class="table-preview-size" aria-live="polite">
            {{ previewSize ? `${previewSize.rows} 行 × ${previewSize.columns} 列` : '行列数无效' }}
          </span>
        </div>
        <div class="table-preview" role="region" aria-label="表格结构预览" tabindex="0">
          <table v-if="previewSize" class="table-preview-grid" aria-label="待插入的表格结构"
                 :style="{width: `${previewSize.columns * 64}px`}">
            <thead>
              <tr>
                <th v-for="column in previewSize.columns" :key="column" scope="col">表头 {{ column }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in previewSize.rows - 1" :key="row">
                <td v-for="column in previewSize.columns" :key="column"/>
              </tr>
            </tbody>
          </table>
          <div v-else class="table-preview-message">请输入有效的行数和列数</div>
        </div>
        <div class="table-description">仅预览结构，插入的单元格为空；可在预览区内滚动查看</div>
      </q-card-section>
      <q-card-actions align="right" class="table-dialog-actions">
        <q-btn flat label="取消" color="primary" v-close-popup/>
        <q-btn unelevated label="插入" color="primary" :disable="!valid" @click="confirm"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<style scoped>
.table-insert-dialog {
  width: 380px;
  max-width: calc(100vw - 32px);
  max-height: calc(100vh - 48px);
  max-height: calc(100dvh - 48px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color);
}
.table-dialog-header, .table-dialog-actions { flex: 0 0 auto; }
.table-dialog-body { min-height: 0; overflow-y: auto; }
.table-description { color: var(--pad-text-color-400); font-size: 12px; margin-top: 8px; }
.table-dimension { display: flex; align-items: center; gap: 8px; margin-bottom: 16px; }
.table-dimension > span { flex: 1; }
.table-dimension .q-input { width: 90px; }
.table-dimension :deep(input) { text-align: center; color: var(--pad-text-color); }
.table-preview-heading { display: flex; justify-content: space-between; gap: 8px; margin: 16px 0 8px; }
.table-preview-size { font-size: 12px; color: var(--pad-text-color-300); align-self: center; }
.table-preview {
  height: 184px;
  box-sizing: border-box;
  overflow: auto;
  overscroll-behavior: contain;
  border: 1px solid var(--pad-border-color);
  border-radius: var(--pad-radius-md);
  background: var(--pad-bg-color-100);
}
.table-preview:focus-visible { outline: 2px solid var(--pad-primary-color); outline-offset: 2px; }
.table-preview-grid { min-width: 100%; table-layout: fixed; border-collapse: separate; border-spacing: 0; }
.table-preview-grid th, .table-preview-grid td {
  box-sizing: border-box;
  height: 32px;
  padding: 4px 8px;
  border-right: 1px solid var(--pad-border-color);
  border-bottom: 1px solid var(--pad-border-color);
  font-size: 12px;
  color: var(--pad-text-color);
  text-align: left;
}
.table-preview-grid th { position: sticky; top: 0; background: var(--pad-bg-color-300); white-space: nowrap; }
.table-preview-message { height: 100%; display: grid; place-items: center; color: var(--pad-text-color-300); font-size: 12px; }
</style>
