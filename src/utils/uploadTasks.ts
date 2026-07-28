import type {AttachmentProcessEvent} from '../bindings';

export type UploadTaskStatus =
  | 'queued'
  | 'pending'
  | 'uploading'
  | 'canceling'
  | 'canceled'
  | 'completed'
  | 'error';

export type UploadTaskPhase = 'preparing' | 'transferring' | 'finalizing';

export interface UploadTask {
  id: string;
  filename: string;
  progress: number;
  status: UploadTaskStatus;
  phase: UploadTaskPhase;
  error?: string;
}

export function createUploadTask(id: string, filename: string): UploadTask {
  return {
    id,
    filename,
    progress: 0,
    status: 'pending',
    phase: 'preparing',
  };
}

export function createQueuedUploadTask(id: string, filename: string): UploadTask {
  return {
    ...createUploadTask(id, filename),
    status: 'queued',
  };
}

export function isUploadTaskTerminal(task: UploadTask): boolean {
  return task.status === 'completed'
    || task.status === 'canceled'
    || task.status === 'error';
}

export function hasActiveUploadTasks(tasks: UploadTask[]): boolean {
  return tasks.some(task => !isUploadTaskTerminal(task));
}

export function applyUploadTaskEvent(task: UploadTask, message: AttachmentProcessEvent): boolean {
  if (isUploadTaskTerminal(task)) {
    const completedAfterCancellation = task.status === 'canceled'
      && (message.event === 'completed' || message.event === 'completedWithoutData');
    if (!completedAfterCancellation) return false;
  }

  switch (message.event) {
    case 'started':
      if (task.status !== 'canceling') {
        task.status = 'uploading';
        task.phase = 'transferring';
      }
      break;
    case 'progress':
      if (task.status !== 'canceling') {
        task.progress = message.data / 100;
      }
      break;
    case 'finalizing':
      if (task.status !== 'canceling') {
        task.phase = 'finalizing';
        task.progress = Math.max(task.progress, 0.99);
      }
      break;
    case 'completed':
    case 'completedWithoutData':
      task.status = 'completed';
      task.progress = 1;
      delete task.error;
      break;
    case 'error':
      if (task.status === 'canceling') {
        task.status = 'canceled';
      } else {
        task.status = 'error';
        task.error = message.data;
      }
      break;
  }
  return true;
}

export function markUploadTaskFailed(task: UploadTask, error: unknown): void {
  if (task.status === 'canceling' || task.status === 'canceled') {
    task.status = 'canceled';
    delete task.error;
    return;
  }
  task.status = 'error';
  task.error = String(error);
}

export function uploadTaskStatusText(task: UploadTask): string {
  if (task.status === 'completed') return '已完成';
  if (task.status === 'canceled') return '已取消';
  if (task.status === 'canceling') return '正在取消';
  if (task.status === 'error') return task.error ? `失败：${task.error}` : '上传失败';
  if (task.phase === 'finalizing') return '正在完成：提交附件并保存日记';
  if (task.status === 'queued') return '等待上传';
  if (task.status === 'pending') return '准备中';
  return `上传中 ${Math.round(task.progress * 100)}%`;
}
