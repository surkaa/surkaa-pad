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
      <q-card-section>
        <div class="text-h6">插入表格</div>
        <div class="table-description">第一行为表头，行数包含表头</div>
      </q-card-section>
      <q-card-section class="q-pt-none">
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
      </q-card-section>
      <q-card-actions align="right">
        <q-btn flat label="取消" color="primary" v-close-popup/>
        <q-btn unelevated label="插入" color="primary" :disable="!valid" @click="confirm"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<style scoped>
.table-insert-dialog { width: 380px; max-width: calc(100vw - 32px); background: var(--pad-bg-color-200); color: var(--pad-text-color); }
.table-description { color: var(--pad-text-color-400); font-size: 12px; margin-top: 8px; }
.table-dimension { display: flex; align-items: center; gap: 8px; margin-bottom: 16px; }
.table-dimension > span { flex: 1; }
.table-dimension .q-input { width: 90px; }
.table-dimension :deep(input) { text-align: center; color: var(--pad-text-color); }
</style>
