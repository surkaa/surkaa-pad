<template>
  <div v-if="loading" class="archive-entry-state" role="status" aria-live="polite">
    <q-icon name="unarchive" color="primary" size="36px"/>
    <strong>{{ phaseLabel }}</strong>
    <PreviewProgressBar class="archive-entry-progress" rounded size="8px" :value="progressValue" :instant-feedback="progress?.phase !== 'extracting'"/>
    <span>
      {{ Math.floor(progressValue * 100) }}%
      <template v-if="progress?.phase === 'extracting' && progress.totalBytes > 0">
        · {{ formatBytes(progress.processedBytes) }} / {{ formatBytes(progress.totalBytes) }}
      </template>
    </span>
    <span>已读取压缩数据 {{ formatBytes(readBytes) }}</span>
    <span v-if="solid">固实压缩需处理目标文件所在的完整压缩块</span>
    <q-btn flat label="取消加载" color="primary" @click="emit('back')"/>
  </div>
  <div v-else-if="passwordRequired" class="archive-entry-state">
    <q-icon name="lock" color="primary" size="36px"/>
    <strong>{{ invalidPassword ? '压缩包密码不正确' : '该文件已加密' }}</strong>
    <q-input v-model="password" outlined dense autofocus type="password" label="压缩包密码" :error="invalidPassword" @keyup.enter="retry"/>
    <span>密码仅用于本次解压，不保存或同步</span>
    <q-btn color="primary" label="重新加载" :disable="!password" @click="retry"/>
  </div>
  <div v-else-if="error" class="archive-entry-state archive-entry-error">
    <q-icon name="error_outline" size="36px"/>
    <span>{{ error }}</span>
    <q-btn flat label="返回目录" color="primary" @click="emit('back')"/>
  </div>
  <FilePreviewContent v-else-if="resource" ref="contentPreview" :resource="resource"/>
</template>

<script setup lang="ts">
import {computed, onBeforeUnmount, ref, shallowRef, watch} from 'vue';
import {Channel} from '@tauri-apps/api/core';
import type {ArchiveEntryEvent, ArchiveEntryProgress, PreparedArchiveEntry} from '../bindings';
import type {ArchiveTreeNode} from '../utils/archivePreview';
import api from '../utils/api';
import {formatBytes} from '../utils/format';
import {formatError} from '../utils/formatError';
import {archiveEntryKind, archiveEntryProgressValue} from '../utils/archiveEntryPreview';
import FilePreviewContent from './FilePreviewContent.vue';
import PreviewProgressBar from './PreviewProgressBar.vue';

const props = defineProps<{diaryId: string; attachmentId: string; entry: ArchiveTreeNode; revision: string; solid?: boolean}>();
const emit = defineEmits<{(event: 'back'): void; (event: 'prepared', id: string): void}>();
const loading = ref(false);
const passwordRequired = ref(false);
const invalidPassword = ref(false);
const password = ref('');
const progress = shallowRef<ArchiveEntryProgress | null>(null);
const readBytes = ref(0);
const resource = shallowRef<PreparedArchiveEntry>();
const contentPreview = ref<InstanceType<typeof FilePreviewContent>>();
const canCopy = computed(() => contentPreview.value?.canCopy ?? false);
const copyReady = computed(() => contentPreview.value?.copyReady ?? false);
defineExpose({canCopy, copyReady, copy: () => contentPreview.value?.copy()});
const error = ref('');
let task = '';
let generation = 0;
const progressValue = computed(() => archiveEntryProgressValue(progress.value));
const phaseLabel = computed(() => ({preparing: '正在准备读取…', extracting: '正在解压目标文件…', verifying: '正在校验文件…'}[progress.value?.phase || 'preparing']));

watch(() => [props.diaryId, props.attachmentId, props.entry.sourceIndex, props.revision] as const, () => void load(), {immediate: true});
onBeforeUnmount(() => cancel(false));

function cancel(remove: boolean) {
  generation++;
  if (task) void api.cmdCancelTask(task).catch(() => undefined);
  task = '';
  if (resource.value) void api.cmdReleaseArchiveEntry(resource.value.id, remove).catch(() => undefined);
  resource.value = undefined;
}

async function load(secret?: string) {
  cancel(false);
  const current = generation;
  error.value = '';
  passwordRequired.value = false;
  invalidPassword.value = false;
  progress.value = null;
  readBytes.value = 0;
  password.value = '';
  loading.value = false;
  if (props.entry.sourceIndex === null || !archiveEntryKind({filename: props.entry.label, mimetype: ''})) {
    error.value = '暂不支持预览此类文件或嵌套压缩包'; return;
  }
  if (props.entry.size > 256 * 1024 ** 2) {error.value = '单个文件超过 256 MiB 预览上限，请下载后查看'; return;}
  loading.value = true;
  let terminal = false;
  const event = new Channel<ArchiveEntryEvent>();
  event.onmessage = message => {
    if (current !== generation) {
      if (message.event === 'completed') void api.cmdReleaseArchiveEntry(message.data.resource.id, true).catch(() => undefined);
      return;
    }
    if (message.event === 'started') return;
    if (message.event === 'reading') {readBytes.value = message.data.bytes; return;}
    if (message.event === 'progress') {progress.value = message.data.progress; return;}
    terminal = true; task = ''; loading.value = false;
    if (message.event === 'completed') {resource.value = message.data.resource; emit('prepared', resource.value.id);}
    else if (message.event === 'passwordRequired') {passwordRequired.value = true; invalidPassword.value = message.data.invalidPassword;}
    else if (message.event === 'error') error.value = message.data.message;
    else if (message.event === 'cancelled') error.value = '已取消加载';
  };
  try {
    const token = await api.cmdPrepareArchiveEntry({diaryId: props.diaryId, attachmentId: props.attachmentId,
      index: props.entry.sourceIndex, path: props.entry.path, size: props.entry.size, revision: props.revision}, secret || null, event);
    if (current !== generation) void api.cmdCancelTask(token).catch(() => undefined);
    else if (!terminal) task = token;
  } catch (cause) {
    if (current !== generation || terminal) return;
    loading.value = false; error.value = formatError(cause);
  }
}

function retry() {if (password.value && !loading.value) void load(password.value);}
</script>

<style scoped>
.archive-entry-state {display: flex; flex: 1; min-height: 0; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 24px; color: var(--pad-text-color-300); text-align: center;}
.archive-entry-progress {width: min(460px, 100%);}
.archive-entry-error {color: var(--q-negative);}
</style>
