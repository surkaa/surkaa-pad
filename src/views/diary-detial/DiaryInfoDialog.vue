<template>
  <q-dialog no-refocus :model-value="modelValue" @update:model-value="emit('update:modelValue', $event)">
    <q-card class="diary-detail-dialog">
      <q-card-section class="row items-center q-pb-none">
        <div class="text-h6">{{ diary?.title }} - 详情</div>
        <q-space/>
        <q-btn icon="close" flat round dense v-close-popup/>
      </q-card-section>

      <q-card-section class="q-pa-md diary-detail-dialog-content">
        <div class="text-subtitle2 q-mb-xs">基本信息</div>
        <div class="text-caption diary-id-row">
          <span>ID：</span>
          <code class="diary-id-value">{{ diaryId }}</code>
        </div>
        <div class="text-caption">创建时间：{{ formatTimestamp(diary?.created) }}</div>
        <div class="text-caption">更新时间：{{ formatTimestamp(diary?.updated) }}</div>

        <q-separator class="q-my-md"/>

        <div class="text-subtitle2 q-mb-sm">附件列表 ({{ attachments.length }})</div>
        <q-list v-if="attachments.length" bordered class="attachment-groups-list">
          <q-expansion-item
            v-for="group in attachmentGroups"
            :key="group.type"
            v-model="expandedAttachmentGroups[group.type]"
            :icon="attachmentGroupIcon(group.type)"
            :label="`${group.type} (${group.attachments.length})`"
            header-class="attachment-group-header"
          >
            <q-list v-if="expandedAttachmentGroups[group.type]" separator>
              <AttachmentCard
                v-for="attachment in group.attachments"
                :key="attachment.id"
                :att="attachment"
              />
            </q-list>
          </q-expansion-item>
        </q-list>
        <div v-else class="text-center q-pa-sm">暂无附件</div>
      </q-card-section>

      <q-card-actions align="right">
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, ref} from 'vue';
import type {AttachmentMeta, DiarySummary} from '../../bindings';
import AttachmentCard from '../../components/AttachmentCard.vue';
import {formatTimestamp} from '../../utils';
import {attachmentGroupIcon, groupAttachmentsByMimeType} from '../../utils/attachmentGrouping';

const props = defineProps<{
  modelValue: boolean;
  diary?: DiarySummary;
  diaryId: string;
  attachments: AttachmentMeta[];
}>();
const emit = defineEmits<{(event: 'update:modelValue', value: boolean): void}>();
const attachmentGroups = computed(() => groupAttachmentsByMimeType(props.attachments));
const expandedAttachmentGroups = ref<Record<string, boolean>>({});
</script>

<style scoped lang="scss">
.diary-detail-dialog {
  width: min(640px, 92vw);
  max-height: 90vh;
}

.diary-detail-dialog-content {
  max-height: calc(90vh - 112px);
  overflow-y: auto;
}

.diary-id-row {
  display: flex;
  align-items: baseline;
}

.diary-id-value {
  color: var(--pad-text-color-200);
  overflow-wrap: anywhere;
  user-select: all;
}

.attachment-groups-list {
  border-color: var(--pad-border-color) !important;
}

:deep(.attachment-group-header) {
  color: var(--pad-text-color-200);
}
</style>
