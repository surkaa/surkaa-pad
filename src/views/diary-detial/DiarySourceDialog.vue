<template>
  <q-dialog
    no-refocus
    persistent
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="diary-source-dialog-card">
      <q-card-section class="row items-center q-pb-sm">
        <div class="text-h6">完整 Manifest</div>
        <q-space/>
        <q-btn icon="close" flat round dense v-close-popup aria-label="关闭源码弹窗"/>
      </q-card-section>
      <q-separator/>
      <q-card-section class="diary-source-content">
        <div v-if="loading" class="column items-center justify-center full-height q-gutter-sm">
          <q-spinner color="primary" size="32px"/>
          <div class="text-caption diary-source-loading-text">正在读取完整 Manifest...</div>
        </div>
        <pre v-else class="diary-manifest-source">{{ manifestSource }}</pre>
      </q-card-section>
      <q-separator/>
      <q-card-actions align="right">
        <q-btn
          flat
          icon="content_copy"
          label="复制完整 Manifest"
          color="primary"
          :disable="loading || !manifestSource"
          @click="copyManifest"
        />
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {useQuasar} from 'quasar';
import {ref, watch} from 'vue';
import api from '../../utils/api';
import {copyTextToClipboard} from '../../utils/clipboard';
import {formatError} from '../../utils/formatError';

const props = defineProps<{modelValue: boolean; diaryId: number}>();
const emit = defineEmits<{(event: 'update:modelValue', value: boolean): void}>();
const $q = useQuasar();
const manifestSource = ref('');
const loading = ref(false);

watch(() => props.modelValue, async visible => {
  if (!visible) return;
  loading.value = true;
  manifestSource.value = '';
  try {
    const manifest = await api.cmdGetDiaryManifest(props.diaryId);
    manifestSource.value = JSON.stringify(manifest, null, 2);
  } catch (error) {
    emit('update:modelValue', false);
    $q.notify({type: 'negative', message: `加载完整 Manifest 失败：${formatError(error)}`});
  } finally {
    loading.value = false;
  }
});

async function copyManifest() {
  if (!manifestSource.value) return;
  try {
    await copyTextToClipboard(manifestSource.value);
    $q.notify({type: 'positive', message: '完整 Manifest 已复制'});
  } catch (error) {
    $q.notify({type: 'negative', message: `复制 Manifest 失败：${formatError(error)}`});
  }
}
</script>

<style scoped lang="scss">
.diary-source-dialog-card {
  display: flex;
  flex-direction: column;
  width: min(900px, 94vw);
  height: min(720px, 86vh);
}

.diary-source-content {
  flex: 1;
  min-height: 0;
  padding: 0;
}

.diary-source-loading-text {
  color: var(--pad-text-color-300);
}

.diary-manifest-source {
  width: 100%;
  height: 100%;
  margin: 0;
  padding: 16px;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-200);
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
}
</style>
