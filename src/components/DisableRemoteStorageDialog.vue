<template>
  <q-dialog :model-value="modelValue" persistent @update:model-value="emit('update:modelValue', $event)">
    <q-card class="disable-remote-dialog">
      <q-card-section>
        <div class="text-h6 dialog-title">关闭云同步</div>
        <div class="text-caption dialog-description">
          关闭后应用将使用本地数据，云端对象不会自动删除。
        </div>
      </q-card-section>

      <q-card-section v-if="loading" class="loading-state q-pt-none">
        <q-spinner color="primary" size="2em"/>
        <span>正在统计云端数据和本地可用空间…</span>
      </q-card-section>

      <q-card-section v-else-if="plan" class="plan-details q-pt-none">
        <div class="detail-row">
          <span>云端有效数据</span>
          <strong>{{ plan.remoteFiles }} 个文件 · {{ formatBytes(plan.remoteBytes) }}</strong>
        </div>
        <div class="detail-row">
          <span>本地已有并跳过</span>
          <strong>{{ plan.skippedFiles }} 个文件 · {{ formatBytes(plan.skippedBytes) }}</strong>
        </div>
        <div class="detail-row">
          <span>本次需要下载</span>
          <strong>{{ plan.downloadFiles }} 个文件 · {{ formatBytes(plan.downloadBytes) }}</strong>
        </div>
        <div class="detail-row">
          <span>本地可用空间</span>
          <strong :class="{'insufficient-text': !plan.hasSufficientSpace}">
            {{ formatBytes(plan.availableBytes) }}
          </strong>
        </div>
        <div class="path-detail">
          <span>保存位置</span>
          <strong>{{ plan.localStoragePath }}</strong>
        </div>

        <div v-if="!plan.hasSufficientSpace" class="space-warning">
          <q-icon name="warning" size="20px"/>
          <span>为避免下载后磁盘空间过低，当前无法开始下载。请手动删除一些大附件或释放磁盘空间后重试。</span>
        </div>
        <div v-else-if="plan.downloadBytes > 0" class="download-tip">
          下载期间请保持应用运行；数据量较大时可能需要较长时间。
        </div>
      </q-card-section>

      <q-card-actions align="right" class="q-px-md q-pb-md">
        <q-btn flat label="取消" class="secondary-action" :disable="loading" @click="close"/>
        <q-btn
          unelevated
          :label="plan?.downloadFiles ? '下载并关闭' : '关闭云同步'"
          color="primary"
          :disable="loading || !plan || !plan.hasSufficientSpace"
          @click="emit('confirm')"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import type {DisableRemoteStoragePlan} from '../bindings';
import {formatBytes} from '../utils/format';

defineProps<{
  modelValue: boolean;
  loading: boolean;
  plan?: DisableRemoteStoragePlan;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: boolean];
  confirm: [];
  cancel: [];
}>();

function close() {
  emit('update:modelValue', false);
  emit('cancel');
}
</script>

<style scoped lang="scss">
.disable-remote-dialog {
  width: min(520px, calc(100vw - 32px));
  max-width: 520px;
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-200);
  border-radius: var(--pad-radius-xl);
}

.dialog-title {
  color: var(--pad-text-color-100);
}

.dialog-description,
.download-tip {
  color: var(--pad-text-color-400);
}

.loading-state {
  min-height: 96px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--pad-text-color-300);
}

.plan-details {
  display: grid;
  gap: 10px;
}

.detail-row {
  display: flex;
  justify-content: space-between;
  gap: 18px;

  span {
    color: var(--pad-text-color-400);
  }

  strong {
    color: var(--pad-text-color-200);
    text-align: right;
  }
}

.path-detail {
  display: grid;
  gap: 4px;
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--pad-bg-color-200);

  span {
    color: var(--pad-text-color-400);
  }

  strong {
    overflow-wrap: anywhere;
    color: var(--pad-text-color-300);
  }
}

.space-warning {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  border-radius: 10px;
  color: var(--q-negative);
  background: color-mix(in srgb, var(--q-negative) 12%, transparent);
}

.insufficient-text {
  color: var(--q-negative) !important;
}

.secondary-action {
  color: var(--pad-text-color-400);
}

@media (max-width: 600px) {
  .detail-row {
    display: grid;
    gap: 2px;

    strong {
      text-align: left;
    }
  }
}
</style>
