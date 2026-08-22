<template>
  <JsonSourceDialog
    persistent
    :model-value="modelValue"
    title="完整 Manifest"
    :source="manifest"
    :loading="loading"
    loading-text="正在读取完整 Manifest…"
    copy-label="复制完整 Manifest"
    copy-success-message="完整 Manifest 已复制"
    copy-error-prefix="复制 Manifest 失败"
    @update:model-value="emit('update:modelValue', $event)"
  />
</template>

<script setup lang="ts">
import {useQuasar} from 'quasar';
import {ref, watch} from 'vue';
import type {DiaryManifest} from '../../bindings';
import JsonSourceDialog from '../../components/JsonSourceDialog.vue';
import api from '../../utils/api';
import {formatError} from '../../utils/formatError';

const props = defineProps<{modelValue: boolean; diaryId: string}>();
const emit = defineEmits<{(event: 'update:modelValue', value: boolean): void}>();
const $q = useQuasar();
const manifest = ref<DiaryManifest>();
const loading = ref(false);

watch(() => props.modelValue, async visible => {
  if (!visible) return;
  loading.value = true;
  manifest.value = undefined;
  try {
    manifest.value = await api.cmdGetDiaryManifest(props.diaryId);
  } catch (error) {
    emit('update:modelValue', false);
    $q.notify({type: 'negative', message: `加载完整 Manifest 失败：${formatError(error)}`});
  } finally {
    loading.value = false;
  }
});
</script>
