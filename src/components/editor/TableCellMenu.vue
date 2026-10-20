<script setup lang="ts">
import {computed} from 'vue';
import {useQuasar} from 'quasar';
import type {TableAction} from './tableCommands';

export interface TableMenuItem {action: TableAction | 'copy'; label: string; enabled: boolean}
const props = defineProps<{
  modelValue: boolean;
  mobile: boolean;
  anchor: HTMLElement | null;
  items: TableMenuItem[];
}>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'select', action: TableMenuItem['action']): void;
}>();
const $q = useQuasar();
const visible = computed({get: () => props.modelValue, set: value => emit('update:modelValue', value)});
function select(item: TableMenuItem) {
  if (!item.enabled) return;
  visible.value = false;
  emit('select', item.action);
}
</script>

<template>
  <q-dialog v-if="mobile" v-model="visible" position="bottom" no-refocus>
    <q-card class="table-cell-menu table-mobile-menu">
      <div class="q-pa-md text-subtitle1">单元格操作</div>
      <q-list dense class="table-menu-list">
        <q-item v-for="item in items" :key="item.action" clickable :disable="!item.enabled"
                :class="{'text-negative': item.action.startsWith('delete')}" @click="select(item)">
          <q-item-section>{{ item.label }}</q-item-section>
        </q-item>
      </q-list>
      <q-card-actions align="right"><q-btn flat label="关闭" color="primary" v-close-popup/></q-card-actions>
    </q-card>
  </q-dialog>
  <q-menu v-else v-model="visible" :target="anchor || false" no-parent-event no-refocus
          :dark="$q.dark.isActive" class="table-cell-menu" :offset="[0, 2]" max-height="70vh">
    <q-list dense class="table-menu-list">
      <q-item v-for="item in items" :key="item.action" clickable :disable="!item.enabled"
              :class="{'text-negative': item.action.startsWith('delete')}" @click="select(item)">
        <q-item-section>{{ item.label }}</q-item-section>
      </q-item>
    </q-list>
  </q-menu>
</template>

<style scoped>
.table-cell-menu { min-width: 200px; background: var(--pad-bg-color-200); color: var(--pad-text-color); }
.table-mobile-menu { width: 100%; max-height: 75dvh; display: flex; flex-direction: column; overflow: hidden; }
.table-mobile-menu .table-menu-list { min-height: 0; overflow-y: auto; overscroll-behavior: contain; }
.table-mobile-menu .q-card-actions { flex-shrink: 0; }
</style>
