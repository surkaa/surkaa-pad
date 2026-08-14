import {describe, expect, it} from 'vitest';
import {
  applyUploadTaskEvent,
  createQueuedUploadTask,
  createUploadTask,
  hasActiveUploadTasks,
  isUploadTaskProgressIndeterminate,
  isUploadTaskTerminal,
  markUploadTaskFailed,
  uploadTasksDialogTitle,
  uploadTaskStatusText,
} from '../uploadTasks';

describe('upload task domain model', () => {
  it('shows a newly queued task as waiting', () => {
    const task = createQueuedUploadTask('queued', 'queued.mp4');

    expect(task.status).toBe('queued');
    expect(uploadTaskStatusText(task)).toBe('等待上传');
  });

  it('uses download wording for attachment cache tasks', () => {
    const task = createUploadTask('download', 'archive.zip', 'download');

    expect(uploadTaskStatusText(task)).toBe('准备中');
    applyUploadTaskEvent(task, {event: 'started'});
    applyUploadTaskEvent(task, {event: 'progress', data: 42});
    expect(uploadTaskStatusText(task)).toBe('下载中 42%');
    applyUploadTaskEvent(task, {event: 'finalizing'});
    expect(uploadTaskStatusText(task)).toBe('即将完成：写入本地缓存');

    const queued = createQueuedUploadTask('queued-download', 'video.mp4', 'download');
    expect(uploadTaskStatusText(queued)).toBe('等待下载');
    queued.status = 'error';
    expect(uploadTaskStatusText(queued)).toBe('下载失败');
  });

  it('tracks transfer and finalization progress', () => {
    const task = createUploadTask('task-1', 'video.mp4');

    expect(applyUploadTaskEvent(task, {event: 'started'})).toBe(true);
    expect(isUploadTaskProgressIndeterminate(task)).toBe(true);
    expect(applyUploadTaskEvent(task, {event: 'progress', data: 58})).toBe(true);
    expect(task).toMatchObject({status: 'uploading', phase: 'transferring', progress: 0.58});
    expect(isUploadTaskProgressIndeterminate(task)).toBe(false);

    applyUploadTaskEvent(task, {event: 'finalizing'});
    expect(task).toMatchObject({phase: 'finalizing', progress: 0.99});
    expect(isUploadTaskProgressIndeterminate(task)).toBe(true);
    expect(uploadTaskStatusText(task)).toBe('即将完成：提交附件并保存日记');

    applyUploadTaskEvent(task, {event: 'completedWithoutData'});
    expect(task).toMatchObject({status: 'completed', progress: 1});
    expect(isUploadTaskProgressIndeterminate(task)).toBe(false);
    expect(isUploadTaskTerminal(task)).toBe(true);
  });

  it('keeps the exact backend error on a failed item', () => {
    const task = createUploadTask('task-2', 'archive.zip');
    applyUploadTaskEvent(task, {event: 'error', data: 'disk full'});

    expect(task.status).toBe('error');
    expect(task.error).toBe('disk full');
    expect(uploadTaskStatusText(task)).toBe('失败：disk full');
  });

  it('turns an error caused during cancellation into canceled', () => {
    const task = createUploadTask('task-3', 'movie.mp4');
    task.status = 'canceling';

    applyUploadTaskEvent(task, {event: 'error', data: 'request aborted'});
    expect(task.status).toBe('canceled');
    expect(task.error).toBeUndefined();
  });

  it('accepts a committed completion that arrives just after cancellation', () => {
    const task = createUploadTask('task-4', 'photo.jpg');
    task.status = 'canceled';

    expect(applyUploadTaskEvent(task, {event: 'completedWithoutData'})).toBe(true);
    expect(task.status).toBe('completed');
    expect(applyUploadTaskEvent(task, {event: 'error', data: 'late error'})).toBe(false);
    expect(task.status).toBe('completed');
  });

  it('reports active tasks until every item reaches a terminal state', () => {
    const completed = createUploadTask('completed', 'a.jpg');
    completed.status = 'completed';
    const failed = createUploadTask('failed', 'b.jpg');
    markUploadTaskFailed(failed, 'network error');
    const pending = createUploadTask('pending', 'c.jpg');

    expect(hasActiveUploadTasks([completed, failed, pending])).toBe(true);
    pending.status = 'canceled';
    expect(hasActiveUploadTasks([completed, failed, pending])).toBe(false);
  });

  it('summarizes the task count and terminal state in the dialog title', () => {
    const completed = createUploadTask('completed-title', 'a.jpg');
    completed.status = 'completed';
    const canceled = createUploadTask('canceled-title', 'b.jpg');
    canceled.status = 'canceled';
    const failed = createUploadTask('failed-title', 'c.jpg');
    failed.status = 'error';

    expect(uploadTasksDialogTitle([completed, canceled, createUploadTask('active', 'd.jpg')]))
      .toBe('3个文件正在处理中');
    expect(uploadTasksDialogTitle([completed, canceled, failed])).toBe('3个文件已完成');
  });
});
