<template>
  <q-dialog
    no-refocus
    :persistent="persistent"
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="json-source-card">
      <q-card-section class="row items-center q-pb-sm">
        <div class="text-h6">{{ title }}</div>
        <q-space/>
        <q-btn icon="close" flat round dense v-close-popup :aria-label="`关闭${title}弹窗`"/>
      </q-card-section>
      <q-separator/>
      <div v-if="!loading && source !== undefined" class="json-source-toolbar">
        <q-btn
          v-if="hasNestedJsonObjectString"
          flat
          dense
          no-caps
          icon="data_object"
          color="primary"
          :label="expandJsonStrings ? '显示原始字符串' : '解析 JSON 字符串'"
          @click="toggleJsonStrings"
        />
        <q-space/>
        <q-btn
          flat
          round
          dense
          icon="unfold_more"
          color="primary"
          aria-label="展开全部 JSON 节点"
          @click="setExpansion('all')"
        >
          <q-tooltip>展开全部</q-tooltip>
        </q-btn>
        <q-btn
          flat
          round
          dense
          icon="unfold_less"
          color="primary"
          aria-label="收起全部 JSON 节点"
          @click="setExpansion('root')"
        >
          <q-tooltip>收起全部</q-tooltip>
        </q-btn>
      </div>
      <q-separator v-if="!loading && source !== undefined"/>
      <q-card-section class="json-source-content">
        <div v-if="loading" class="column items-center justify-center full-height q-gutter-sm">
          <q-spinner color="primary" size="32px"/>
          <div class="text-caption json-source-loading-text">{{ loadingText }}</div>
        </div>
        <div v-else-if="source !== undefined" class="json-source-tree">
          <VueJsonPretty
            :key="viewerRevision"
            :data="displaySource"
            :deep="viewerDepth"
            :theme="jsonTheme"
            show-icon
            show-line
            collapsed-on-click-brackets
          />
        </div>
      </q-card-section>
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
import VueJsonPretty from 'vue-json-pretty';
import 'vue-json-pretty/lib/styles.css';
import {copyTextToClipboard} from '../utils/clipboard';
import {formatError} from '../utils/formatError';
import {
  containsNestedJsonObjectString,
  expandNestedJsonObjectStrings,
  formatJsonSource,
} from '../utils/jsonSource';

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
const expandJsonStrings = ref(props.expandJsonStringsByDefault);
const viewerDepth = ref(3);
const viewerRevision = ref(0);

type JsonSourceValue = string | number | boolean | unknown[] | Record<string, unknown> | null;

const hasNestedJsonObjectString = computed(() => (
  props.source !== undefined && containsNestedJsonObjectString(props.source)
));
const displaySource = computed<JsonSourceValue>(() => {
  const source = expandJsonStrings.value
    ? expandNestedJsonObjectStrings(props.source)
    : props.source;
  return source as JsonSourceValue;
});
const jsonTheme = computed(() => $q.dark.isActive ? 'dark' : 'light');
const rawSourceText = computed(() => (
  props.source === undefined ? '' : formatJsonSource(props.source)
));

watch(() => props.modelValue, visible => {
  if (!visible) return;
  expandJsonStrings.value = props.expandJsonStringsByDefault;
  setExpansion(3);
});

function toggleJsonStrings() {
  expandJsonStrings.value = !expandJsonStrings.value;
  setExpansion(3);
}

function setExpansion(mode: 'all' | 'root' | number) {
  viewerDepth.value = mode === 'all' ? Number.POSITIVE_INFINITY : mode === 'root' ? 1 : mode;
  viewerRevision.value += 1;
}

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
  height: min(760px, 88vh);
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-100);
}

.json-source-toolbar {
  display: flex;
  align-items: center;
  min-height: 42px;
  padding: 3px 10px;
  background: var(--pad-bg-color-200);
}

.json-source-content {
  flex: 1;
  min-height: 0;
  padding: 0;
  background: var(--pad-bg-color-100);
}

.json-source-tree {
  width: 100%;
  height: 100%;
  padding: 12px 14px;
  overflow: auto;
  color: var(--pad-text-color-200);
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
}

.json-source-tree :deep(.vjs-tree) {
  color: var(--pad-text-color-200);
  font-family: inherit;
  font-size: inherit;
}

.json-source-tree :deep(.vjs-tree-node) {
  min-height: 22px;
  line-height: 22px;
}

.json-source-tree :deep(.vjs-tree-node:hover),
.json-source-tree :deep(.vjs-tree-node.dark:hover) {
  background: color-mix(in srgb, var(--q-primary) 10%, transparent);
}

.json-source-tree :deep(.vjs-key),
.json-source-tree :deep(.vjs-value-number),
.json-source-tree :deep(.vjs-value-boolean) {
  color: var(--q-primary);
}

.json-source-tree :deep(.vjs-value-string) {
  color: var(--pad-text-color-200);
}

.json-source-tree :deep(.vjs-value-null),
.json-source-tree :deep(.vjs-value-undefined),
.json-source-tree :deep(.vjs-comment),
.json-source-tree :deep(.vjs-carets) {
  color: var(--pad-text-color-400);
}

.json-source-tree :deep(.vjs-indent-unit.has-line) {
  border-color: var(--pad-bg-color-400);
}

.json-source-loading-text {
  color: var(--pad-text-color-300);
}

@media (max-width: 600px) {
  .json-source-card {
    width: 96vw;
    height: 90vh;
  }

  .json-source-toolbar {
    padding-inline: 6px;
  }
}
</style>
