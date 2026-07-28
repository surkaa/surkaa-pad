import {describe, expect, it} from 'vitest';
import {
  applyUploadTaskEvent,
  createUploadTask,
  hasActiveUploadTasks,
  isUploadTaskTerminal,
  markUploadTaskFailed,
  uploadTaskStatusText,
} from '../uploadTasks';

describe('upload task domain model', () => {
  it('tracks transfer and finalization progress', () => {
    const task = createUploadTask('task-1', 'video.mp4');

    expect(applyUploadTaskEvent(task, {event: 'started'})).toBe(true);
    expect(applyUploadTaskEvent(task, {event: 'progress', data: 58})).toBe(true);
    expect(task).toMatchObject({status: 'uploading', phase: 'transferring', progress: 0.58});

    applyUploadTaskEvent(task, {event: 'finalizing'});
    expect(task).toMatchObject({phase: 'finalizing', progress: 0.99});

    applyUploadTaskEvent(task, {event: 'completedWithoutData'});
    expect(task).toMatchObject({status: 'completed', progress: 1});
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
});
