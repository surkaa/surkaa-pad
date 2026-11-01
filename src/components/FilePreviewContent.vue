<template>
  <div v-if="loading || error" class="file-preview-state" :class="{'file-preview-error': error}">
    <q-spinner v-if="loading" color="primary" size="36px"/>
    <span>{{ error || '正在读取预览…' }}</span>
  </div>
  <PdfAttachmentPreview v-else-if="kind === 'pdf'" :url="buildPdfPreviewUrl(resource.url)"/>
  <JsonlAttachmentPreview v-else-if="kind === 'jsonl'" :key="resource.url" :url="resource.url"/>
  <ImagePreview v-else-if="kind === 'image'" :src="resource.url" embedded/>
  <AudioFilePreview v-else-if="kind === 'audio'" :key="resource.url" :url="resource.url" :size="resource.size"/>
  <div v-else-if="kind === 'video'" class="file-preview-media">
    <video :src="resource.url" controls playsinline preload="metadata" @error="error = '视频无法解码，请使用支持此格式的播放器'"/>
  </div>
  <div v-else-if="kind === 'html'" class="file-preview-state">
    <q-icon name="html" size="48px" color="primary"/>
    <span>HTML 将在外部浏览器运行，可执行其中的脚本。</span>
    <span>仅提供当前文件；依赖压缩包内其他文件的页面可能不完整。</span>
    <q-btn color="primary" label="在浏览器中打开" @click="openHtml"/>
  </div>
  <template v-else-if="kind === 'json'">
    <q-banner v-if="jsonError" dense class="file-preview-warning">JSON 格式错误，已按文本显示：{{ jsonError }}</q-banner>
    <JsonTreeViewer v-if="jsonParsed" :source="json"/>
    <pre v-else class="file-preview-text">{{ text }}</pre>
  </template>
  <div v-else-if="kind === 'markdown'" class="file-preview-markdown" @click="openLink" v-html="renderSafeMarkdown(text)"/>
  <pre v-else-if="kind === 'text'" class="file-preview-text">{{ text }}</pre>
  <div v-else class="file-preview-state">暂不支持预览此类文件</div>
</template>

<script setup lang="ts">
import {computed, defineAsyncComponent, onBeforeUnmount, ref, shallowRef, watch} from 'vue';
import {useQuasar} from 'quasar';
import {openUrl} from '@tauri-apps/plugin-opener';
import type {PreparedArchiveEntry} from '../bindings';
import {archiveEntryKind} from '../utils/archiveEntryPreview';
import {buildPdfPreviewUrl, fetchAttachmentText} from '../utils/attachmentPreview';
import {formatError} from '../utils/formatError';
import {renderSafeMarkdown} from '../utils/aiMarkdown';
import {copyTextToClipboard} from '../utils/clipboard';
import api from '../utils/api';
import JsonTreeViewer from './JsonTreeViewer.vue';
import JsonlAttachmentPreview from './JsonlAttachmentPreview.vue';
import ImagePreview from './ImagePreview.vue';
import AudioFilePreview from './AudioFilePreview.vue';
const PdfAttachmentPreview = defineAsyncComponent(() => import('./PdfAttachmentPreview.vue'));
const props = defineProps<{resource: PreparedArchiveEntry}>();
const $q = useQuasar();
const kind = computed(() => archiveEntryKind(props.resource));
const canCopy = computed(() => ['json', 'markdown', 'text'].includes(kind.value || ''));
const loading = ref(false); const error = ref(''); const text = ref('');
const copyReady = computed(() => canCopy.value && !loading.value && !error.value);
defineExpose({canCopy, copyReady, copy, dismissOverlay});
const json = shallowRef<unknown>(); const jsonParsed = ref(false); const jsonError = ref('');
let controller: AbortController | undefined;
let htmlConfirmation: ReturnType<typeof $q.dialog> | undefined;
watch(() => props.resource, async resource => {
  controller?.abort(); const active = new AbortController(); controller = active;
  loading.value = false; error.value = ''; text.value = ''; json.value = undefined; jsonParsed.value = false; jsonError.value = '';
  if (!canCopy.value) return;
  loading.value = true;
  try {
    const result = await fetchAttachmentText(resource.url, resource.size, active.signal);
    if (active.signal.aborted) return;
    text.value = result;
    if (kind.value === 'json') {
      try {json.value = JSON.parse(result); jsonParsed.value = true;} catch (cause) {jsonError.value = formatError(cause);}
    }
  } catch (cause) {if (!active.signal.aborted) error.value = formatError(cause);}
  finally {if (!active.signal.aborted) loading.value = false;}
}, {immediate: true});
onBeforeUnmount(() => {controller?.abort(); htmlConfirmation?.hide();});

async function copy() {
  if (!copyReady.value) return;
  try {await copyTextToClipboard(text.value); $q.notify({type: 'positive', message: '文件内容已复制'});}
  catch (cause) {$q.notify({type: 'negative', message: formatError(cause)});}
}
function openHtml() {
  if (htmlConfirmation) return;
  const resourceId = props.resource.id;
  const confirmation = $q.dialog({title: '打开 HTML 文件', message: '只打开你信任的文件。页面中的 JavaScript 可以访问网络；请勿在其中输入主密码等敏感信息。',
    cancel: true, ok: {label: '信任并打开', color: 'primary'}, noBackdropDismiss: true});
  htmlConfirmation = confirmation;
  confirmation.onOk(async () => {
    try {await api.cmdOpenArchiveEntryHtml(resourceId);} catch (cause) {error.value = formatError(cause);}
  }).onDismiss(() => {
    if (htmlConfirmation === confirmation) htmlConfirmation = undefined;
  });
}
function dismissOverlay(): boolean {
  if (!htmlConfirmation) return false;
  htmlConfirmation.hide();
  return true;
}
async function openLink(event: MouseEvent) {
  const anchor = (event.target as HTMLElement | null)?.closest<HTMLAnchorElement>('a[href]');
  if (!anchor) return; event.preventDefault();
  try {await openUrl(anchor.href);} catch (cause) {$q.notify({type: 'negative', message: formatError(cause)});}
}
</script>

<style scoped lang="scss">
.file-preview-state, .file-preview-media {display: flex; flex: 1; min-height: 0; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 24px; color: var(--pad-text-color-300); text-align: center;}
.file-preview-media {padding: 0; background: var(--pad-preview-backdrop); overflow: hidden; video {width: 100%; height: 100%; min-height: 0; object-fit: contain;}}
.file-preview-error {color: var(--q-negative);}
.file-preview-warning {color: var(--pad-warning-color); background: color-mix(in srgb, var(--pad-warning-color) 12%, var(--pad-bg-color-200));}
.file-preview-text, .file-preview-markdown {flex: 1; min-height: 0; overflow: auto; padding: 18px 22px; margin: 0; color: var(--pad-text-color-200); background: var(--pad-bg-color-100); user-select: text; -webkit-user-select: text; overflow-wrap: anywhere; line-height: 1.65;}
.file-preview-text {white-space: pre-wrap; font: 14px/1.65 ui-monospace, SFMono-Regular, Consolas, monospace;}
.file-preview-markdown :deep(*) {max-width: 100%;}
</style>
