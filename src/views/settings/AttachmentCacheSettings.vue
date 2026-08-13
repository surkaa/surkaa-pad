<template>
  <q-item class="settings-item">
    <q-item-section avatar class="settings-icon-section">
      <q-icon name="storage"/>
    </q-item-section>
    <q-item-section>
      <q-item-label class="label-text text-weight-medium">本地附件缓存</q-item-label>
      <q-item-label caption class="desc-text">
        <template v-if="info">
          已缓存 {{ info.cachedFiles }} 个附件 · {{ formatBytes(info.cachedBytes) }} / {{ formatBytes(info.limitBytes) }}
        </template>
        <template v-else-if="loading">正在统计本地附件缓存…</template>
        <template v-else>限制云同步模式保留在本地的附件总大小</template>
      </q-item-label>
    </q-item-section>
    <q-item-section side>
      <q-select
        :model-value="info?.limitBytes"
        :options="ATTACHMENT_CACHE_LIMIT_OPTIONS"
        :display-value="info ? attachmentCacheLimitLabel(info.limitBytes) : '—'"
        emit-value
        map-options
        dense
        outlined
        options-dense
        popup-content-class="pad-attachment-cache-menu"
        aria-label="本地附件缓存上限"
        class="cache-limit-select"
        :loading="loading"
        :disable="loading"
        @update:model-value="updateLimit"
      />
    </q-item-section>
  </q-item>
  <q-item class="settings-item">
    <q-item-section avatar class="settings-icon-section">
      <q-icon name="cloud_download"/>
    </q-item-section>
    <q-item-section>
      <q-item-label class="label-text text-weight-medium">单个附件缓存上限</q-item-label>
      <q-item-label caption class="desc-text">
        超过此大小仍会上传并可正常访问，但不保留本地副本
      </q-item-label>
    </q-item-section>
    <q-item-section side>
      <q-select
        :model-value="info?.maxFileSizeBytes"
        :options="ATTACHMENT_CACHE_FILE_SIZE_OPTIONS"
        :display-value="info ? attachmentCacheFileSizeLabel(info.maxFileSizeBytes) : '—'"
        emit-value
        map-options
        dense
        outlined
        options-dense
        popup-content-class="pad-attachment-cache-menu"
        aria-label="单个附件缓存上限"
        class="cache-limit-select"
        :loading="loading"
        :disable="loading"
        @update:model-value="updateMaxFileSize"
      />
    </q-item-section>
  </q-item>
</template>

<script setup lang="ts">
import {onMounted, ref} from 'vue';
import {useQuasar} from 'quasar';
import type {AttachmentCacheInfo} from '../../bindings';
import api from '../../utils/api';
import {
  ATTACHMENT_CACHE_LIMIT_OPTIONS,
  ATTACHMENT_CACHE_FILE_SIZE_OPTIONS,
  attachmentCacheFileSizeLabel,
  attachmentCacheLimitLabel,
} from '../../utils/attachmentCache';
import {formatBytes} from '../../utils/format';
import {formatError} from '../../utils/formatError';

const $q = useQuasar();
const loading = ref(false);
const info = ref<AttachmentCacheInfo>();

async function refresh() {
  loading.value = true;
  try {
    info.value = await api.cmdGetAttachmentCacheInfo();
  } catch (error) {
    $q.notify({type: 'negative', message: `读取附件缓存统计失败：${formatError(error)}`});
  } finally {
    loading.value = false;
  }
}

async function updateLimit(limitBytes: number | null) {
  if (limitBytes == null || limitBytes === info.value?.limitBytes) return;
  loading.value = true;
  try {
    info.value = await api.cmdSetAttachmentCacheLimit(limitBytes);
    $q.notify({type: 'positive', message: '本地附件缓存上限已更新'});
  } catch (error) {
    $q.notify({type: 'negative', message: `更新附件缓存上限失败：${formatError(error)}`});
  } finally {
    loading.value = false;
  }
}

async function updateMaxFileSize(limitBytes: number | null) {
  if (limitBytes == null || limitBytes === info.value?.maxFileSizeBytes) return;
  loading.value = true;
  try {
    info.value = await api.cmdSetAttachmentCacheMaxFileSize(limitBytes);
    $q.notify({type: 'positive', message: '单个附件缓存上限已更新'});
  } catch (error) {
    $q.notify({type: 'negative', message: `更新单个附件缓存上限失败：${formatError(error)}`});
  } finally {
    loading.value = false;
  }
}

onMounted(refresh);
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>

<style scoped lang="scss">
.cache-limit-select {
  width: 104px;

  :deep(.q-field__native),
  :deep(.q-field__input) {
    color: var(--pad-text-color-200) !important;
  }

  :deep(.q-field__marginal) {
    color: var(--pad-text-color-300) !important;
  }
}
</style>
