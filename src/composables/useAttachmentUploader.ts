import {Channel} from '@tauri-apps/api/core';
import {useQuasar} from 'quasar';
import {computed, onUnmounted, type Ref, ref} from 'vue';
import {v4 as uuidv4} from 'uuid';
import type {AttachmentMeta, AttachmentProcessEvent} from '../bindings';
import {useDataStore} from '../stores/data';
import api from '../utils/api';
import {formatError} from '../utils/formatError';
import {
  applyUploadTaskEvent,
  createQueuedUploadTask,
  createUploadTask,
  hasActiveUploadTasks,
  isUploadTaskTerminal,
  markUploadTaskFailed,
  type UploadTask,
} from '../utils/uploadTasks';

export type {UploadTask} from '../utils/uploadTasks';

const CHUNK_SIZE = 5 * 1024 * 1024;

type AttachmentProcessSuccess = (meta: AttachmentMeta, url: string) => void;
type TaskCanceler = () => Promise<boolean | void>;

interface TaskController {
  cancelerPromise: Promise<TaskCanceler | null>;
  resolveCanceler: (canceler: TaskCanceler | null) => void;
  cancelerResolved: boolean;
  settledPromise: Promise<void>;
  resolveSettled: () => void;
  settled: boolean;
  cancellationPromise?: Promise<boolean>;
  onCanceled?: () => void;
  cancellationNotified?: boolean;
}

function createTaskController(): TaskController {
  let resolveCanceler!: (canceler: TaskCanceler | null) => void;
  let resolveSettled!: () => void;
  const cancelerPromise = new Promise<TaskCanceler | null>(resolve => {
    resolveCanceler = resolve;
  });
  const settledPromise = new Promise<void>(resolve => {
    resolveSettled = resolve;
  });
  return {
    cancelerPromise,
    resolveCanceler,
    cancelerResolved: false,
    settledPromise,
    resolveSettled,
    settled: false,
  };
}

function isCancellationRequested(task: UploadTask): boolean {
  return task.status === 'canceling' || task.status === 'canceled';
}

export function useAttachmentUploader(diaryId: Ref<string>) {
  const $q = useQuasar();
  const dataStore = useDataStore();
  const taskControllers = new Map<string, TaskController>();
  let cancelAllRequested = false;
  const uploadTaskMap = ref<Record<string, UploadTask>>({});
  const showUploadDialog = ref(false);
  const uploadTasks = computed(() => Object.values(uploadTaskMap.value));
  const hasActiveUploads = computed(() => hasActiveUploadTasks(uploadTasks.value));
  const allUploadsSettled = computed(() => !hasActiveUploads.value);

  function createTask(filename: string, queued = false) {
    const key = uuidv4();
    uploadTaskMap.value[key] = queued
      ? createQueuedUploadTask(key, filename)
      : createUploadTask(key, filename);
    taskControllers.set(key, createTaskController());
    return key;
  }

  function resolveTaskCanceler(key: string, canceler: TaskCanceler | null) {
    const controller = taskControllers.get(key);
    if (!controller || controller.cancelerResolved) return;
    controller.cancelerResolved = true;
    controller.resolveCanceler(canceler);
  }

  function settleTask(key: string) {
    const controller = taskControllers.get(key);
    resolveTaskCanceler(key, null);
    if (controller && !controller.settled) {
      controller.settled = true;
      controller.resolveSettled();
    }
    taskControllers.delete(key);
  }

  function notifyTaskCanceled(controller: TaskController) {
    if (controller.cancellationNotified) return;
    controller.cancellationNotified = true;
    controller.onCanceled?.();
  }

  function failTask(
    key: string,
    error: unknown,
    errorCallback?: (errorMessage: string) => void,
    notify = true,
  ) {
    const task = uploadTaskMap.value[key];
    if (!task) return;
    const controller = taskControllers.get(key);
    const message = formatError(error);
    markUploadTaskFailed(task, message);
    if (task.status === 'canceled' && controller) {
      notifyTaskCanceled(controller);
    }
    settleTask(key);
    if (task.status === 'error') {
      if (notify) {
        $q.notify({type: 'negative', message: `${task.filename} 上传失败: ${message}`});
      }
      errorCallback?.(message);
    }
  }

  function createUploadChannel(
    key: string,
    onSuccess?: AttachmentProcessSuccess,
    onError?: (errorMessage: string) => void,
  ) {
    const controller = taskControllers.get(key);
    if (controller) {
      controller.onCanceled = () => onError?.('上传已取消');
    }
    const event = new Channel<AttachmentProcessEvent>();
    event.onmessage = message => {
      const task = uploadTaskMap.value[key];
      if (!task) return;

      if (!applyUploadTaskEvent(task, message)) return;
      switch (message.event) {
        case 'completed': {
          settleTask(key);
          const [meta, url] = message.data;
          dataStore.updateAttachment(diaryId.value, meta);
          onSuccess?.(meta, url);
          break;
        }
        case 'completedWithoutData':
          settleTask(key);
          break;
        case 'error':
          settleTask(key);
          if (task.status === 'error') {
            $q.notify({type: 'negative', message: `${task.filename} 上传失败: ${message.data}`});
            onError?.(message.data);
          }
          break;
      }
    };
    return event;
  }

  function registerTaskCanceler(key: string, canceler: TaskCanceler) {
    const task = uploadTaskMap.value[key];
    if (!task || isUploadTaskTerminal(task)) return;
    resolveTaskCanceler(key, canceler);
  }

  function registerCancelableTask(key: string, token: string) {
    registerTaskCanceler(key, async () => api.cmdCancelTask(token));
  }

  async function cancelUploadTask(key: string): Promise<boolean> {
    const task = uploadTaskMap.value[key];
    if (!task || isUploadTaskTerminal(task)) return true;

    const controller = taskControllers.get(key);
    if (!controller) {
      failTask(key, '找不到上传任务的取消控制器');
      return false;
    }
    if (controller.cancellationPromise) return controller.cancellationPromise;

    if (task.status === 'queued') {
      task.status = 'canceled';
      notifyTaskCanceled(controller);
      settleTask(key);
      return true;
    }

    task.status = 'canceling';
    controller.cancellationPromise = (async () => {
      try {
        const canceler = await controller.cancelerPromise;
        if (!canceler) {
          if (task.status === 'canceled') notifyTaskCanceled(controller);
          return isUploadTaskTerminal(task);
        }
        const canceled = await canceler();
        if (canceled === false && !isUploadTaskTerminal(task)) {
          const outcomeArrived = await Promise.race([
            controller.settledPromise.then(() => true),
            new Promise<boolean>(resolve => setTimeout(() => resolve(false), 1000)),
          ]);
          if (!outcomeArrived) {
            throw new Error('后端任务已经结束，但未收到最终结果');
          }
        }
        if (!isUploadTaskTerminal(task)) {
          task.status = 'canceled';
          delete task.error;
        }
        if (task.status === 'canceled') {
          notifyTaskCanceled(controller);
        }
        settleTask(key);
        return true;
      } catch (error) {
        if (isUploadTaskTerminal(task)) {
          if (task.status === 'canceled') notifyTaskCanceled(controller);
          return true;
        }
        task.status = 'error';
        task.error = `取消失败：${formatError(error)}`;
        settleTask(key);
        $q.notify({type: 'negative', message: `${task.filename} ${task.error}`});
        return false;
      }
    })();
    return controller.cancellationPromise;
  }

  async function cancelAllUploads(): Promise<boolean> {
    cancelAllRequested = true;
    const activeTaskIds = uploadTasks.value
      .filter(task => !isUploadTaskTerminal(task))
      .map(task => task.id);
    const results = await Promise.all(activeTaskIds.map(cancelUploadTask));
    await new Promise(resolve => setTimeout(resolve, 0));
    return results.every(Boolean) && !hasActiveUploads.value;
  }

  function resetUploadTasks(): boolean {
    if (hasActiveUploads.value) return false;
    uploadTaskMap.value = {};
    taskControllers.clear();
    cancelAllRequested = false;
    return true;
  }

  function skipCanceledTaskBeforeStart(
    key: string,
    errorCallback?: (errorMessage: string) => void,
  ): boolean {
    const task = uploadTaskMap.value[key];
    if (!task) throw new Error(`找不到预先登记的上传任务：${key}`);
    if (!cancelAllRequested && task.status !== 'canceled' && task.status !== 'canceling') {
      return false;
    }
    task.status = 'canceled';
    const controller = taskControllers.get(key);
    if (controller) {
      controller.onCanceled = () => errorCallback?.('上传已取消');
      notifyTaskCanceled(controller);
    } else {
      errorCallback?.('上传已取消');
    }
    settleTask(key);
    return true;
  }

  async function uploadAttachment(
    accessPath: string,
    encrypted: boolean,
    completedCallback?: AttachmentProcessSuccess,
    errorCallback?: (errorMessage: string) => void,
    queuedTaskId?: string,
  ) {
    const rawName = accessPath.split(/[\\/]/).pop() || '未知文件';
    const key = queuedTaskId ?? createTask(rawName);
    const event = createUploadChannel(key, completedCallback, errorCallback);
    if (skipCanceledTaskBeforeStart(key, errorCallback)) return;
    const task = uploadTaskMap.value[key];
    if (!task) throw new Error(`找不到预先登记的上传任务：${key}`);
    task.status = 'pending';

    try {
      const token = await api.cmdAddAttachment(
        event,
        diaryId.value,
        accessPath,
        encrypted,
        rawName,
      );
      registerCancelableTask(key, token);
    } catch (error) {
      console.error('调用 Rust 后端失败:', error);
      failTask(key, error, errorCallback);
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
    queuedTaskId?: string,
  ) {
    const key = queuedTaskId ?? createTask(filename);
    const task = uploadTaskMap.value[key];
    if (!task) throw new Error(`找不到预先登记的上传任务：${key}`);
    const controller = taskControllers.get(key);
    if (controller) {
      controller.onCanceled = () => errorCallback?.('上传已取消');
    }
    showUploadDialog.value = true;
    if (skipCanceledTaskBeforeStart(key, errorCallback)) return;

    let uploadToken: string | null = null;
    try {
      task.status = 'uploading';
      task.phase = 'transferring';
      const startResult = await api.cmdStartChunkedUpload(
        diaryId.value,
        filename,
        mimetype,
        encrypted,
        totalSize,
      );
      uploadToken = startResult.uploadToken;
      registerTaskCanceler(key, async () => {
        await api.cmdAbortChunkedUpload(startResult.uploadToken);
      });

      const totalChunks = Math.ceil(totalSize / CHUNK_SIZE);
      for (let index = 0; index < totalChunks; index++) {
        if (isCancellationRequested(task)) {
          await cancelUploadTask(key);
          return;
        }
        const start = index * CHUNK_SIZE;
        const end = Math.min(start + CHUNK_SIZE, totalSize);
        const bytes = await readChunk(start, end);
        if (bytes.length !== end - start) {
          throw new Error(`读取分片大小不匹配：expected=${end - start}, actual=${bytes.length}`);
        }
        const chunkResult = await api.cmdUploadChunk(uploadToken, index, Array.from(bytes));
        task.progress = Math.min(chunkResult.uploadedBytes / chunkResult.totalBytes, 0.99);
      }

      if (isCancellationRequested(task)) {
        await cancelUploadTask(key);
        return;
      }
      task.phase = 'finalizing';
      const finishResult = await api.cmdFinishChunkedUpload(uploadToken);
      uploadToken = null;
      task.status = 'completed';
      task.progress = 1;
      settleTask(key);
      dataStore.updateAttachment(diaryId.value, finishResult.attachment);
      completedCallback?.(finishResult.attachment, finishResult.url);
    } catch (error) {
      if (isCancellationRequested(task)) {
        await cancelUploadTask(key);
        return;
      }
      if (uploadToken) {
        try {
          await api.cmdAbortChunkedUpload(uploadToken);
        } catch (abortError) {
          console.error('清理失败的分片上传会话失败:', abortError);
        }
      }
      console.error('分片上传失败:', error);
      failTask(key, error, errorCallback);
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

  onUnmounted(() => {
    void cancelAllUploads();
  });

  return {
    uploadTaskMap,
    uploadTasks,
    showUploadDialog,
    hasActiveUploads,
    allUploadsSettled,
    createTask,
    createUploadChannel,
    failTask,
    registerCancelableTask,
    resetUploadTasks,
    cancelUploadTask,
    cancelAllUploads,
    uploadAttachment,
    uploadAttachmentChunked,
    uploadMemoryAttachmentChunked,
  };
}
