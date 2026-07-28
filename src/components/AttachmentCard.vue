<script setup lang="ts">
import {formatBytes} from "../utils";
import {AttachmentMeta} from "../bindings.ts";

defineProps<{
  att: AttachmentMeta
}>();
</script>

<template>
  <q-item>
    <q-item-section>
      <q-item-label class="text-weight-medium attachment-filename">{{ att.filename }}</q-item-label>
      <q-item-label caption class="attachment-meta">
        {{ att.mimetype }} · {{ formatBytes(att.size) }}
      </q-item-label>
    </q-item-section>
    <q-item-section side>
      <q-chip
          :color="att.encrypted ? 'orange-2' : 'green-2'"
          :text-color="att.encrypted ? 'orange-9' : 'green-9'"
          size="sm"
          dense
      >
        {{ att.encrypted ? '已加密' : '明文' }}
      </q-chip>
    </q-item-section>
  </q-item>
</template>

<style scoped lang="scss">
.q-item,
:deep(.q-item__section--main) {
  min-width: 0;
}

.attachment-filename,
.attachment-meta {
  white-space: normal;
  overflow-wrap: anywhere;
  word-break: break-word;
}

.attachment-meta {
  color: var(--pad-text-color-300) !important;
}
</style>
