<template>
  <q-dialog
    no-refocus
    persistent
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="unused-attachments-dialog">
      <q-card-section>
        <div class="text-h6">未使用的附件</div>
        <div class="q-mt-sm text-body2">
          有 {{ count }} 个附件没有出现在正文中，请选择处理方式。
        </div>
      </q-card-section>
      <q-card-actions align="right" class="unused-attachment-actions">
        <q-btn flat label="保留" color="primary" :disable="loading" @click="emit('keep')"/>
        <q-btn unelevated label="添加到日记末尾" color="primary" :loading="loading" @click="emit('append')"/>
        <q-btn flat label="删除附件" color="negative" :disable="loading" @click="emit('delete')"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
defineProps<{modelValue: boolean; count: number; loading: boolean}>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'keep' | 'append' | 'delete'): void;
}>();
</script>

<style scoped>
.unused-attachments-dialog {
  width: min(440px, 90vw);
}

.unused-attachment-actions {
  gap: 4px;
}
</style>
