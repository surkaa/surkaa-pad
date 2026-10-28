// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, ref, type App} from 'vue';
import {useAttachmentUploader} from '../useAttachmentUploader';

const mocks = vi.hoisted(() => ({notify: vi.fn(), cancel: vi.fn()}));
vi.mock('quasar', () => ({useQuasar: () => ({notify: mocks.notify})}));
vi.mock('../../stores/data', () => ({useDataStore: () => ({updateAttachment: vi.fn()})}));
vi.mock('../../utils/api', () => ({default: {cmdCancelTask: mocks.cancel}}));
vi.mock('@tauri-apps/api/core', () => ({Channel: class {onmessage = () => {};}}));
let app: App | undefined;
function mount() {
  let uploader!: ReturnType<typeof useAttachmentUploader>;
  const host = document.createElement('div'); document.body.append(host);
  app = createApp({setup() { uploader = useAttachmentUploader(ref('diary')); return () => h('div'); }});
  app.mount(host); return uploader;
}
afterEach(() => { app?.unmount(); app = undefined; document.body.innerHTML = ''; vi.clearAllMocks(); });

describe('目录选择取消的下载任务收尾', () => {
  it('收到导出完成和位置句柄后结清任务，但不当成上传更新编辑器链接', async () => {
    const uploader = mount();
    const key = uploader.createTask('批量下载', false, 'download');
    const success = vi.fn(); const channel = uploader.createUploadChannel(key, success);
    channel.onmessage({event: 'completedWithDownloadLocation', data: 'saved-location'});
    expect(uploader.uploadTaskMap.value[key]).toMatchObject({status: 'completed', downloadLocationToken: 'saved-location'});
    expect(uploader.allUploadsSettled.value).toBe(true);
    expect(success).not.toHaveBeenCalled();
    expect(await uploader.cancelUploadTask(key)).toBe(true); expect(mocks.cancel).not.toHaveBeenCalled();
    expect(uploader.resetUploadTasks()).toBe(true);
  });

  it('关闭选择器后结清任务，允许下次下载，不产生失败通知', async () => {
    const uploader = mount();
    const key = uploader.createTask('批量下载', false, 'download');
    expect(uploader.hasActiveUploads.value).toBe(true);
    uploader.cancelUnstartedTask(key);
    expect(uploader.uploadTaskMap.value[key].status).toBe('canceled');
    expect(uploader.allUploadsSettled.value).toBe(true);
    expect(await uploader.cancelUploadTask(key)).toBe(true);
    expect(uploader.resetUploadTasks()).toBe(true);
    expect(mocks.cancel).not.toHaveBeenCalled(); expect(mocks.notify).not.toHaveBeenCalled();
  });

  it('取消请求先于选择器关闭时也会解除等待，不调用不存在的后端任务', async () => {
    const uploader = mount();
    const key = uploader.createTask('批量下载', false, 'download');
    const canceled = uploader.cancelUploadTask(key);
    expect(uploader.uploadTaskMap.value[key].status).toBe('canceling');
    uploader.cancelUnstartedTask(key);
    expect(await canceled).toBe(true);
    expect(uploader.allUploadsSettled.value).toBe(true);
    expect(mocks.cancel).not.toHaveBeenCalled(); expect(mocks.notify).not.toHaveBeenCalled();
  });

  it('重复关闭或失效任务不改变已完成任务状态', () => {
    const uploader = mount();
    const key = uploader.createTask('批量下载', false, 'download');
    uploader.uploadTaskMap.value[key].status = 'completed';
    uploader.cancelUnstartedTask(key); uploader.cancelUnstartedTask('不存在');
    expect(uploader.uploadTaskMap.value[key].status).toBe('completed');
  });
});
