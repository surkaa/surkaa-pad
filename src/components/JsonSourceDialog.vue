<template>
  <q-dialog
    no-refocus
    :persistent="persistent"
    :maximized="isMaximized"
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="json-source-card" :class="{'is-maximized': isMaximized}">
      <q-card-section class="json-source-heading row items-center no-wrap q-pb-sm">
        <div class="json-source-title text-h6">{{ title }}</div>
        <q-space/>
        <DialogMaximizeButton v-if="!autoMaximized" :maximized="isMaximized" @toggle="toggleMaximized"/>
        <q-btn icon="close" flat round dense v-close-popup :aria-label="`关闭${title}弹窗`"/>
      </q-card-section>
      <q-separator/>
      <q-card-section v-if="loading" class="json-source-content">
        <div class="column items-center justify-center full-height q-gutter-sm">
          <q-spinner color="primary" size="32px"/>
          <div class="text-caption json-source-loading-text">{{ loadingText }}</div>
        </div>
      </q-card-section>
      <JsonTreeViewer
        v-else-if="source !== undefined"
        :key="viewerOpenRevision"
        :source="source"
        :expand-json-strings-by-default="expandJsonStringsByDefault"
      />
      <q-separator/>
      <q-card-actions align="right">
        <q-btn
          flat
          icon="content_copy"
          :label="copyLabel"
          color="primary"
          :disable="loading || !rawSourceText"
          @click="copySource"
        />
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import {copyTextToClipboard} from '../utils/clipboard';
import {formatError} from '../utils/formatError';
import {formatJsonSource} from '../utils/jsonSource';
import JsonTreeViewer from './JsonTreeViewer.vue';
import DialogMaximizeButton from './DialogMaximizeButton.vue';
import {useDialogMaximized} from '../composables/useDialogMaximized';

const props = withDefaults(defineProps<{
  modelValue: boolean;
  title: string;
  source?: unknown;
  loading?: boolean;
  loadingText?: string;
  copyLabel?: string;
  copySuccessMessage?: string;
  copyErrorPrefix?: string;
  persistent?: boolean;
  expandJsonStringsByDefault?: boolean;
}>(), {
  source: undefined,
  loading: false,
  loadingText: '正在读取 JSON…',
  copyLabel: '复制 JSON',
  copySuccessMessage: 'JSON 已复制',
  copyErrorPrefix: '复制 JSON 失败',
  persistent: false,
  expandJsonStringsByDefault: true,
});
const emit = defineEmits<{(event: 'update:modelValue', value: boolean): void}>();
const $q = useQuasar();
const {autoMaximized, isMaximized, toggleMaximized} = useDialogMaximized(() => props.modelValue);
const viewerOpenRevision = ref(0);
const rawSourceText = computed(() => (
  props.source === undefined ? '' : formatJsonSource(props.source)
));

watch(() => props.modelValue, visible => {
  if (!visible) return;
  viewerOpenRevision.value += 1;
});

async function copySource() {
  if (!rawSourceText.value) return;
  try {
    await copyTextToClipboard(rawSourceText.value);
    $q.notify({type: 'positive', message: props.copySuccessMessage});
  } catch (error) {
    $q.notify({type: 'negative', message: `${props.copyErrorPrefix}：${formatError(error)}`});
  }
}
</script>

<style scoped lang="scss">
.json-source-card {
  display: flex;
  flex-direction: column;
  width: min(960px, 94vw);
  max-width: calc(100vw - 48px);
  height: min(760px, 88vh);
  overflow: hidden;
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-100);
}

.json-source-card.is-maximized {
  width: 100%;
  height: 100%;
  max-width: none;
  max-height: none;
  border-radius: 0;
}

.json-source-heading,
.json-source-card > .q-card__actions,
.json-source-card > .q-separator {
  flex-shrink: 0;
}

.json-source-title {
  min-width: 0;
  overflow-wrap: anywhere;
}

.json-source-heading :deep(.q-btn) {
  flex-shrink: 0;
}

.json-source-content {
  flex: 1;
  min-height: 0;
  padding: 0;
  background: var(--pad-bg-color-100);
}

.json-source-loading-text {
  color: var(--pad-text-color-300);
}
</style>
