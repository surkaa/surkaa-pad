<template>
  <div class="json-tree-viewer">
    <div class="json-tree-toolbar">
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
    <q-separator/>
    <div class="json-tree-content">
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
  </div>
</template>

<script setup lang="ts">
import {computed, ref} from 'vue';
import {useQuasar} from 'quasar';
import VueJsonPretty from 'vue-json-pretty';
import 'vue-json-pretty/lib/styles.css';
import {
  containsNestedJsonObjectString,
  expandNestedJsonObjectStrings,
} from '../utils/jsonSource';

const props = withDefaults(defineProps<{
  source: unknown;
  expandJsonStringsByDefault?: boolean;
}>(), {
  expandJsonStringsByDefault: true,
});
const $q = useQuasar();
const expandJsonStrings = ref(props.expandJsonStringsByDefault);
const viewerDepth = ref(3);
const viewerRevision = ref(0);

type JsonSourceValue = string | number | boolean | unknown[] | Record<string, unknown> | null;

const hasNestedJsonObjectString = computed(() => containsNestedJsonObjectString(props.source));
const displaySource = computed<JsonSourceValue>(() => {
  const source = expandJsonStrings.value
    ? expandNestedJsonObjectStrings(props.source)
    : props.source;
  return source as JsonSourceValue;
});
const jsonTheme = computed(() => $q.dark.isActive ? 'dark' : 'light');

function toggleJsonStrings() {
  expandJsonStrings.value = !expandJsonStrings.value;
  setExpansion(3);
}

function setExpansion(mode: 'all' | 'root' | number) {
  viewerDepth.value = mode === 'all' ? Number.POSITIVE_INFINITY : mode === 'root' ? 1 : mode;
  viewerRevision.value += 1;
}
</script>

<style scoped lang="scss">
.json-tree-viewer {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  background: var(--pad-bg-color-100);
}

.json-tree-toolbar {
  display: flex;
  align-items: center;
  min-height: 42px;
  padding: 3px 10px;
  background: var(--pad-bg-color-200);
}

.json-tree-content {
  flex: 1;
  min-height: 0;
  padding: 12px 14px;
  overflow: auto;
  color: var(--pad-text-color-200);
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
}

.json-tree-content :deep(.vjs-tree) {
  color: var(--pad-text-color-200);
  font-family: inherit;
  font-size: inherit;
}

.json-tree-content :deep(.vjs-tree-node) {
  min-height: 22px;
  line-height: 22px;
}

.json-tree-content :deep(.vjs-tree-node:hover),
.json-tree-content :deep(.vjs-tree-node.dark:hover) {
  background: color-mix(in srgb, var(--q-primary) 10%, transparent);
}

.json-tree-content :deep(.vjs-key),
.json-tree-content :deep(.vjs-value-number),
.json-tree-content :deep(.vjs-value-boolean) {
  color: var(--q-primary);
}

.json-tree-content :deep(.vjs-value-string) {
  color: var(--pad-text-color-200);
}

.json-tree-content :deep(.vjs-value-null),
.json-tree-content :deep(.vjs-value-undefined),
.json-tree-content :deep(.vjs-comment),
.json-tree-content :deep(.vjs-carets) {
  color: var(--pad-text-color-400);
}

.json-tree-content :deep(.vjs-indent-unit.has-line) {
  border-color: var(--pad-bg-color-400);
}

@media (max-width: 600px) {
  .json-tree-toolbar {
    padding-inline: 6px;
  }
}
</style>
