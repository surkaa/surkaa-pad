<template>
  <q-dialog
    no-refocus
    :maximized="$q.screen.lt.sm"
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="attachment-preview-card">
      <q-card-section class="attachment-preview-header row items-center no-wrap">
        <q-icon :name="previewIcon" size="24px" color="primary"/>
        <div class="attachment-preview-title ellipsis">{{ attachment?.filename || '附件预览' }}</div>
        <q-space/>
        <q-btn flat round dense icon="close" aria-label="关闭附件预览" v-close-popup/>
      </q-card-section>
      <q-separator/>

      <div v-if="loading" class="attachment-preview-state">
        <q-spinner color="primary" size="36px"/>
        <span>正在读取附件…</span>
      </div>
      <div v-else-if="fatalError" class="attachment-preview-state attachment-preview-error">
        <q-icon name="error_outline" size="36px"/>
        <span>{{ fatalError }}</span>
      </div>
      <PdfAttachmentPreview
        v-else-if="modelValue && kind === 'pdf' && url"
        :url="buildPdfPreviewUrl(url)"
      />
      <template v-else-if="kind === 'json'">
        <q-banner v-if="jsonError" class="attachment-preview-warning" dense>
          JSON 格式错误，已按普通文本显示：{{ jsonError }}
        </q-banner>
        <JsonTreeViewer v-if="jsonParsed" :source="jsonSource"/>
        <pre v-else class="attachment-preview-text">{{ sourceText }}</pre>
      </template>
      <div
        v-else-if="kind === 'markdown'"
        class="attachment-preview-markdown"
        @click="openMarkdownLink"
        v-html="renderSafeMarkdown(sourceText)"
      />
      <pre v-else-if="kind === 'text'" class="attachment-preview-text">{{ sourceText }}</pre>
      <div v-else class="attachment-preview-state">
        <span>当前附件暂不支持预览</span>
      </div>

      <q-separator/>
      <q-card-actions align="right" class="attachment-preview-actions">
        <q-btn
          v-if="kind !== 'pdf'"
          flat
          icon="content_copy"
          label="复制内容"
          color="primary"
          :disable="loading || Boolean(fatalError) || !sourceText"
          @click="copyContent"
        />
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, defineAsyncComponent, onBeforeUnmount, ref, shallowRef, watch} from 'vue';
import {useQuasar} from 'quasar';
import {openUrl} from '@tauri-apps/plugin-opener';
import type {AttachmentMeta} from '../bindings';
import {
  attachmentPreviewKind,
  buildPdfPreviewUrl,
  fetchAttachmentText,
  type AttachmentPreviewKind,
} from '../utils/attachmentPreview';
import {renderSafeMarkdown} from '../utils/aiMarkdown';
import {copyTextToClipboard} from '../utils/clipboard';
import {formatError} from '../utils/formatError';
import JsonTreeViewer from './JsonTreeViewer.vue';

const PdfAttachmentPreview = defineAsyncComponent(() => import('./PdfAttachmentPreview.vue'));

const props = defineProps<{
  modelValue: boolean;
  attachment: AttachmentMeta | null;
  url?: string;
}>();
const emit = defineEmits<{(event: 'update:modelValue', value: boolean): void}>();
const $q = useQuasar();
const loading = ref(false);
const sourceText = ref('');
const jsonSource = shallowRef<unknown>();
const jsonParsed = ref(false);
const jsonError = ref('');
const fatalError = ref('');
let loadController: AbortController | null = null;
let loadRevision = 0;

const kind = computed<AttachmentPreviewKind | null>(() => (
  props.attachment ? attachmentPreviewKind(props.attachment) : null
));
const previewIcons: Record<AttachmentPreviewKind, string> = {
  pdf: 'picture_as_pdf',
  markdown: 'markdown',
  json: 'data_object',
  text: 'description',
};
const previewIcon = computed(() => previewIcons[kind.value || 'text']);

watch(
  () => [props.modelValue, props.attachment?.id, props.url] as const,
  ([visible]) => {
    if (visible) void loadPreview();
    else {
      cancelLoad();
      resetPreview();
    }
  },
  {immediate: true},
);

onBeforeUnmount(cancelLoad);

async function loadPreview() {
  cancelLoad();
  const revision = ++loadRevision;
  resetPreview();

  const attachment = props.attachment;
  const attachmentKind = kind.value;
  if (!attachment || !props.url || !attachmentKind) {
    fatalError.value = '当前附件暂不支持预览';
    return;
  }
  if (attachmentKind === 'pdf') return;

  const controller = new AbortController();
  loadController = controller;
  loading.value = true;
  try {
    const text = await fetchAttachmentText(props.url, attachment.size, controller.signal);
    if (revision !== loadRevision) return;
    sourceText.value = text;
    if (attachmentKind === 'json') {
      try {
        jsonSource.value = JSON.parse(text);
        jsonParsed.value = true;
      } catch (error) {
        jsonError.value = formatError(error);
      }
    }
  } catch (error) {
    if (controller.signal.aborted || revision !== loadRevision) return;
    fatalError.value = formatError(error);
  } finally {
    if (revision === loadRevision) loading.value = false;
  }
}

function resetPreview() {
  sourceText.value = '';
  jsonSource.value = undefined;
  jsonParsed.value = false;
  jsonError.value = '';
  fatalError.value = '';
}

function cancelLoad() {
  loadRevision += 1;
  loadController?.abort();
  loadController = null;
  loading.value = false;
}

async function copyContent() {
  try {
    await copyTextToClipboard(sourceText.value);
    $q.notify({type: 'positive', message: '附件内容已复制'});
  } catch (error) {
    $q.notify({type: 'negative', message: `复制附件内容失败：${formatError(error)}`});
  }
}

async function openMarkdownLink(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  const anchor = target?.closest('a[href]') as HTMLAnchorElement | null;
  if (!anchor) return;
  event.preventDefault();
  try {
    await openUrl(anchor.href);
  } catch (error) {
    $q.notify({type: 'negative', message: `打开链接失败：${formatError(error)}`});
  }
}
</script>

<style scoped lang="scss">
.attachment-preview-card {
  display: flex;
  flex-direction: column;
  width: min(1000px, 94vw);
  height: min(820px, 90vh);
  overflow: hidden;
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
}

.attachment-preview-header {
  flex: none;
  gap: 10px;
}

.attachment-preview-title {
  min-width: 0;
  color: var(--pad-text-color-100);
  font-size: 18px;
  font-weight: 600;
}

.attachment-preview-state {
  display: flex;
  flex: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  min-height: 0;
  padding: 24px;
  color: var(--pad-text-color-300);
  text-align: center;
}

.attachment-preview-error {
  color: var(--q-negative);
}

.attachment-preview-warning {
  flex: none;
  color: var(--pad-warning-color);
  background: color-mix(in srgb, var(--pad-warning-color) 12%, var(--pad-bg-color-200));
}

.attachment-preview-text,
.attachment-preview-markdown {
  box-sizing: border-box;
  flex: 1;
  min-height: 0;
  margin: 0;
  padding: 18px 22px;
  overflow: auto;
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-100);
  font-size: 14px;
  line-height: 1.65;
  overflow-wrap: anywhere;
  user-select: text;
  -webkit-user-select: text;
}

.attachment-preview-text {
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  white-space: pre-wrap;
}

.attachment-preview-markdown :deep(*) {
  max-width: 100%;
}

.attachment-preview-markdown :deep(pre) {
  padding: 12px;
  overflow: auto;
  border-radius: 8px;
  background: var(--pad-bg-color-300);
}

.attachment-preview-markdown :deep(code) {
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
}

.attachment-preview-markdown :deep(table) {
  border-collapse: collapse;
}

.attachment-preview-markdown :deep(th),
.attachment-preview-markdown :deep(td) {
  padding: 6px 10px;
  border: 1px solid var(--pad-border-color-100);
}

.attachment-preview-actions {
  flex: none;
}

@media (max-width: 600px) {
  .attachment-preview-card {
    width: 100%;
    height: 100%;
    border-radius: 0;
  }

  .attachment-preview-text,
  .attachment-preview-markdown {
    padding: 14px 16px;
  }
}
</style>
