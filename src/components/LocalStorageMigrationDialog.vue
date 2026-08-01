<template>
  <q-dialog :model-value="modelValue" persistent>
    <q-card class="migration-dialog">
      <q-card-section>
        <div class="text-h6 dialog-title">
          {{ display.completed ? '迁移完成' : display.error ? '迁移失败' : title }}
        </div>
        <div class="text-caption dialog-description">{{ display.statusText }}</div>
      </q-card-section>

      <q-card-section class="q-pt-none">
        <div v-if="display.currentFile" class="current-file ellipsis">
          {{ display.currentFile }}
        </div>
        <div v-if="display.fileDetail" class="text-caption dialog-description q-mb-sm">
          {{ display.fileDetail }}
        </div>

        <q-linear-progress
          v-if="!display.completed && !display.error && display.total > 0"
          :value="Math.min(display.progress / display.total, 1)"
          color="primary"
          rounded
          instant-feedback
          class="q-mt-sm"
        />
        <q-spinner
          v-else-if="!display.completed && !display.error"
          color="primary"
          size="2em"
        />

        <div v-if="display.targetPath" class="path-block q-mt-md">
          <div class="text-caption dialog-description">目标位置</div>
          <div class="path-text">{{ display.targetPath }}</div>
        </div>

        <q-banner v-if="display.error" rounded class="error-banner q-mt-md">
          {{ display.error }}
        </q-banner>
        <q-banner v-if="display.cleanupWarning" rounded class="warning-banner q-mt-md">
          {{ display.cleanupWarning }}
        </q-banner>
      </q-card-section>

      <q-card-actions v-if="display.error" align="right" class="q-px-md q-pb-md">
        <q-btn v-if="allowDefer" flat label="稍后处理" class="secondary-action" @click="$emit('defer')"/>
        <q-btn unelevated label="重试" color="primary" @click="$emit('retry')"/>
      </q-card-actions>
      <q-card-actions v-else-if="display.completed" align="right" class="q-px-md q-pb-md">
        <q-btn unelevated label="重启应用" color="primary" @click="$emit('restart')"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import type {LocalStorageMigrationDisplay} from '../utils/localStorageMigration';

withDefaults(defineProps<{
  modelValue: boolean;
  display: LocalStorageMigrationDisplay;
  title?: string;
  allowDefer?: boolean;
}>(), {
  title: '迁移本地数据',
  allowDefer: false,
});

defineEmits<{
  retry: [];
  defer: [];
  restart: [];
}>();
</script>

<style scoped lang="scss">
.migration-dialog {
  width: min(430px, calc(100vw - 32px));
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-100);
  border-radius: var(--pad-radius-xl);
}

.dialog-title,
.current-file,
.path-text {
  color: var(--pad-text-color-200);
}

.dialog-description {
  color: var(--pad-text-color-400);
}

.current-file,
.path-text {
  font-size: 0.86rem;
}

.path-text {
  overflow-wrap: anywhere;
}

.error-banner {
  background: color-mix(in srgb, var(--pad-danger-color) 16%, var(--pad-bg-color-200));
  color: var(--pad-text-color-200);
}

.warning-banner {
  background: color-mix(in srgb, var(--pad-warning-color) 16%, var(--pad-bg-color-200));
  color: var(--pad-text-color-200);
}

.secondary-action {
  color: var(--pad-text-color-300);
}
</style>
