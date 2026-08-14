<template>
  <q-dialog
    no-refocus
    persistent
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card style="min-width: 300px; max-width: 500px">
      <q-card-section class="row items-center q-pb-none">
        <div class="text-h6">{{ uploadTasksDialogTitle(tasks) }}</div>
      </q-card-section>
      <q-card-section class="q-pt-md">
        <q-list dense>
          <q-item v-for="task in tasks" :key="task.id" class="q-px-none">
            <q-item-section>
              <q-item-label class="text-caption ellipsis upload-task-filename">
                {{ task.filename }}
              </q-item-label>
              <q-item-label caption class="upload-task-status">
                {{ uploadTaskStatusText(task) }}
              </q-item-label>
              <q-linear-progress
                :value="task.progress"
                :indeterminate="isUploadTaskProgressIndeterminate(task)"
                :color="task.status === 'error' ? 'negative' : (task.status === 'canceled' ? 'grey' : 'primary')"
                class="q-mt-sm"
                :animation-speed="200"
              />
            </q-item-section>
            <q-item-section side>
              <q-btn
                v-if="!isUploadTaskTerminal(task)"
                flat
                round
                dense
                icon="close"
                color="grey"
                :loading="task.status === 'canceling'"
                aria-label="取消该任务"
                @click="emit('cancel', task.id)"
              />
              <q-icon v-else :name="uploadTaskIcon(task)" :color="uploadTaskColor(task)"/>
            </q-item-section>
          </q-item>
        </q-list>
      </q-card-section>
      <q-card-actions align="right">
        <q-btn
          v-if="hasActiveTasks"
          flat
          label="全部取消"
          color="negative"
          @click="emit('cancel-all')"
        />
        <q-btn flat label="完成" color="primary" v-close-popup :disable="!allSettled"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed} from 'vue';
import {
  hasActiveUploadTasks,
  isUploadTaskProgressIndeterminate,
  isUploadTaskTerminal,
  type UploadTask,
  uploadTasksDialogTitle,
  uploadTaskStatusText,
} from '../../utils/uploadTasks';

const props = defineProps<{modelValue: boolean; tasks: UploadTask[]; allSettled: boolean}>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'cancel', taskId: string): void;
  (event: 'cancel-all'): void;
}>();
const hasActiveTasks = computed(() => hasActiveUploadTasks(props.tasks));

function uploadTaskIcon(task: UploadTask) {
  if (task.status === 'completed') return 'check_circle';
  if (task.status === 'canceled') return 'cancel';
  return 'error';
}

function uploadTaskColor(task: UploadTask) {
  if (task.status === 'completed') return 'positive';
  if (task.status === 'error') return 'negative';
  return 'grey';
}
</script>

<style scoped>
.upload-task-filename {
  color: var(--pad-text-color-200) !important;
}

.upload-task-status {
  color: var(--pad-text-color-300) !important;
  white-space: normal;
  overflow-wrap: anywhere;
}
</style>
