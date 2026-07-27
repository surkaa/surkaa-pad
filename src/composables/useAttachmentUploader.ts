import {Channel} from '@tauri-apps/api/core';
import {useQuasar} from 'quasar';
import {computed, onUnmounted, type Ref, ref} from 'vue';
import {v4 as uuidv4} from 'uuid';
import type {AttachmentMeta, AttachmentProcessEvent} from '../bindings';
import {useDataStore} from '../stores/data';
import api from '../utils/api';

const CHUNK_SIZE = 5 * 1024 * 1024;

export interface UploadTask {
  filename: string;
  progress: number;
  status: 'pending' | 'uploading' | 'completed' | 'error';
  phase: 'preparing' | 'transferring' | 'finalizing';
}

type AttachmentProcessSuccess = (meta: AttachmentMeta, url: string) => void;

export function useAttachmentUploader(diaryId: Ref<string>) {
  const $q = useQuasar();
  const dataStore = useDataStore();
  const cancelTokens = new Set<string>();
  const chunkedUploadTokens = new Set<string>();
  const uploadTaskMap = ref<Record<string, UploadTask>>({});
  const showUploadDialog = ref(false);
  const uploadTasks = computed(() => Object.values(uploadTaskMap.value));
  const isUploading = computed(() => {
    if (uploadTasks.value.length === 0) return true;
    return uploadTasks.value.every(task => task.status === 'completed' || task.status === 'error');
  });

  function createTask(filename: string) {
    const key = uuidv4();
    uploadTaskMap.value[key] = {
      filename,
      progress: 0,
      status: 'pending',
      phase: 'preparing',
    };
    return key;
  }

  function createUploadChannel(
    key: string,
    onSuccess?: AttachmentProcessSuccess,
    onError?: (errorMessage: string) => void,
  ) {
    const event = new Channel<AttachmentProcessEvent>();
    event.onmessage = message => {
      const task = uploadTaskMap.value[key];
      if (!task) return;

      switch (message.event) {
        case 'started':
          task.status = 'uploading';
          task.phase = 'transferring';
          break;
        case 'progress':
          task.progress = message.data / 100;
          break;
        case 'finalizing':
          task.phase = 'finalizing';
          task.progress = Math.max(task.progress, 0.99);
          break;
        case 'completed': {
          task.status = 'completed';
          task.progress = 1;
          const [meta, url] = message.data;
          dataStore.updateAttachment(diaryId.value, meta);
          onSuccess?.(meta, url);
          break;
        }
        case 'completedWithoutData':
          task.status = 'completed';
          task.progress = 1;
          break;
        case 'error':
          task.status = 'error';
          $q.notify({type: 'negative', message: `${task.filename} 上传失败: ${message.data}`});
          onError?.(message.data);
          break;
      }
    };
    return event;
  }

  function trackCancelableTask(token: string) {
    cancelTokens.add(token);
  }

  async function uploadAttachment(
    accessPath: string,
    encrypted: boolean,
    completedCallback?: AttachmentProcessSuccess,
    errorCallback?: (errorMessage: string) => void,
  ) {
    const rawName = accessPath.split(/[\\/]/).pop() || '未知文件';
    const key = createTask(rawName);
    const event = createUploadChannel(key, completedCallback, errorCallback);

    try {
      trackCancelableTask(await api.cmdAddAttachment(
        event,
        diaryId.value,
        accessPath,
        encrypted,
        rawName,
      ));
    } catch (error) {
      uploadTaskMap.value[key].status = 'error';
      console.error('调用 Rust 后端失败:', error);
      errorCallback?.(String(error));
    }
  }

  async function uploadAttachmentChunked(
    filename: string,
    totalSize: number,
    mimetype: string,
    encrypted: boolean,
    readChunk: (start: number, end: number) => Promise<Uint8Array>,
    completedCallback?: AttachmentProcessSuccess,
    errorCallback?: (errorMessage: string) => void,
  ) {
    const key = createTask(filename);
    showUploadDialog.value = true;

    let uploadToken: string | null = null;
    try {
      uploadTaskMap.value[key].status = 'uploading';
      uploadTaskMap.value[key].phase = 'transferring';
      const startResult = await api.cmdStartChunkedUpload(
        diaryId.value,
        filename,
        mimetype,
        encrypted,
        totalSize,
      );
      uploadToken = startResult.uploadToken;
      chunkedUploadTokens.add(uploadToken);

      const totalChunks = Math.ceil(totalSize / CHUNK_SIZE);
      for (let index = 0; index < totalChunks; index++) {
        const start = index * CHUNK_SIZE;
        const end = Math.min(start + CHUNK_SIZE, totalSize);
        const bytes = await readChunk(start, end);
        if (bytes.length !== end - start) {
          throw new Error(`读取分片大小不匹配：expected=${end - start}, actual=${bytes.length}`);
        }
        const chunkResult = await api.cmdUploadChunk(uploadToken, index, Array.from(bytes));
        uploadTaskMap.value[key].progress = Math.min(
          chunkResult.uploadedBytes / chunkResult.totalBytes,
          0.99,
        );
      }

      uploadTaskMap.value[key].phase = 'finalizing';
      const finishResult = await api.cmdFinishChunkedUpload(uploadToken);
      uploadTaskMap.value[key].status = 'completed';
      uploadTaskMap.value[key].progress = 1;
      chunkedUploadTokens.delete(uploadToken);
      uploadToken = null;
      dataStore.updateAttachment(diaryId.value, finishResult.attachment);
      completedCallback?.(finishResult.attachment, finishResult.url);
    } catch (error) {
      uploadTaskMap.value[key].status = 'error';
      console.error('分片上传失败:', error);
      errorCallback?.(String(error));
      if (uploadToken) {
        chunkedUploadTokens.delete(uploadToken);
        void api.cmdAbortChunkedUpload(uploadToken).catch(() => {});
      }
    }
  }

  function uploadMemoryAttachmentChunked(
    filename: string,
    bytes: Uint8Array,
    mimetype: string,
    encrypted: boolean,
    completedCallback?: AttachmentProcessSuccess,
    errorCallback?: (errorMessage: string) => void,
  ) {
    return uploadAttachmentChunked(
      filename,
      bytes.length,
      mimetype,
      encrypted,
      async (start, end) => bytes.slice(start, end),
      completedCallback,
      errorCallback,
    );
  }

  onUnmounted(async () => {
    const cancellations: Promise<unknown>[] = [
      ...Array.from(cancelTokens, token => api.cmdCancelTask(token)),
      ...Array.from(chunkedUploadTokens, token => api.cmdAbortChunkedUpload(token)),
    ];
    if (cancellations.length > 0) {
      const results = await Promise.allSettled(cancellations);
      for (const result of results) {
        if (result.status === 'rejected') {
          console.error('取消上传任务失败:', result);
        }
      }
    }
    cancelTokens.clear();
    chunkedUploadTokens.clear();
  });

  return {
    uploadTaskMap,
    uploadTasks,
    showUploadDialog,
    isUploading,
    createTask,
    createUploadChannel,
    trackCancelableTask,
    uploadAttachment,
    uploadAttachmentChunked,
    uploadMemoryAttachmentChunked,
  };
}
