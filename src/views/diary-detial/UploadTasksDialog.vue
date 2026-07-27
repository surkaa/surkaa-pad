<template>
  <q-dialog
    no-refocus
    persistent
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card style="min-width: 300px; max-width: 500px">
      <q-card-section class="row items-center q-pb-none">
        <div class="text-h6">文件处理中</div>
      </q-card-section>
      <q-card-section class="q-pt-md">
        <q-list dense>
          <q-item v-for="task in tasks" :key="task.filename" class="q-px-none">
            <q-item-section>
              <q-item-label class="text-caption ellipsis upload-task-filename">
                {{ task.filename }}
              </q-item-label>
              <q-item-label caption class="upload-task-status">
                {{ uploadTaskStatusText(task) }}
              </q-item-label>
              <q-linear-progress
                :value="task.progress"
                :indeterminate="task.phase === 'finalizing' && task.status === 'uploading'"
                :color="task.status === 'error' ? 'negative' : 'primary'"
                class="q-mt-sm"
                :animation-speed="200"
              />
            </q-item-section>
            <q-item-section side>
              <q-icon
                :name="task.status === 'completed' ? 'check_circle' : (task.status === 'error' ? 'error' : 'cloud_upload')"
                :color="task.status === 'completed' ? 'positive' : (task.status === 'error' ? 'negative' : 'grey')"
              />
            </q-item-section>
          </q-item>
        </q-list>
      </q-card-section>
      <q-card-actions align="right">
        <q-btn flat label="完成" color="primary" v-close-popup :disable="!completed"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import type {UploadTask} from '../../composables/useAttachmentUploader';

defineProps<{modelValue: boolean; tasks: UploadTask[]; completed: boolean}>();
const emit = defineEmits<{(event: 'update:modelValue', value: boolean): void}>();

function uploadTaskStatusText(task: UploadTask) {
  if (task.status === 'completed') return '已完成';
  if (task.status === 'error') return '上传失败';
  if (task.phase === 'finalizing') return '正在完成：提交附件并保存日记';
  if (task.status === 'pending') return '准备中';
  return `上传中 ${Math.round(task.progress * 100)}%`;
}
</script>

<style scoped>
.upload-task-filename {
  color: var(--pad-text-color-200) !important;
}

.upload-task-status {
  color: var(--pad-text-color-300) !important;
}
</style>
