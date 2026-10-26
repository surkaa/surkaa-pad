// @vitest-environment happy-dom
import {beforeEach, describe, expect, it, vi} from 'vitest';
import {ref} from 'vue';
import {useMediaAction} from '../useMediaAction';
import {applyUploadTaskEvent, createUploadTask, uploadTaskStatusText} from '../../utils/uploadTasks';

const mocks = vi.hoisted(() => ({
  createTask: vi.fn(), createChannel: vi.fn(), register: vi.fn(),
  save: vi.fn(async () => 'save-token'), toggle: vi.fn(async () => 'toggle-token'),
  rotate: vi.fn(async () => 'rotate-token'),
  saveBatch: vi.fn(), cancelUnstarted: vi.fn(), fail: vi.fn(),
}));
vi.mock('quasar', () => ({useQuasar: () => ({notify: vi.fn()})}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: () => 'windows'}));
vi.mock('../../stores/data', () => ({useDataStore: () => ({})}));
vi.mock('pinia', () => ({storeToRefs: () => ({
  currentDiaryAttachments: ref([{id: 'attachment', filename: '报告.pdf'}]),
  currentDiaryAttachmentUrlMap: ref({}),
})}));
vi.mock('../../stores/config', () => ({useConfigStore: () => ({useTauriConfig: () => ref(false)})}));
vi.mock('../../utils/api', () => ({default: {
  cmdSaveDecryptAttachment: mocks.save,
  cmdSaveDecryptAttachments: mocks.saveBatch,
  cmdToggleAttachmentEncryption: mocks.toggle,
  cmdRotateImageAttachment: mocks.rotate,
}}));
vi.mock('../useAttachmentUploader', () => ({useAttachmentUploader: () => ({
  uploadTasks: ref([]), showUploadDialog: ref(false), hasActiveUploads: ref(false),
  allUploadsSettled: ref(true), createTask: mocks.createTask, createUploadChannel: mocks.createChannel,
  failTask: mocks.fail, cancelUnstartedTask: mocks.cancelUnstarted,
  registerCancelableTask: mocks.register, resetUploadTasks: () => true,
})}));

beforeEach(() => {
  vi.clearAllMocks();
  mocks.createTask.mockReturnValue('task');
  mocks.createChannel.mockReturnValue({});
  mocks.saveBatch.mockReset().mockResolvedValue('batch-token');
});

describe('attachment operation transfer direction', () => {
  it('batch download deduplicates IDs, uses one picker/command and download progress', async () => {
    const action = useMediaAction(ref('diary'), ref(undefined), ref(false), ref(undefined));
    await action.saveDecryptAttachments(['first', 'second', 'first']);
    expect(mocks.createTask).toHaveBeenCalledExactlyOnceWith('批量下载 2 个附件', false, 'download');
    expect(mocks.saveBatch).toHaveBeenCalledExactlyOnceWith({}, 'diary', ['first', 'second']);
    expect(mocks.register).toHaveBeenCalledWith('task', 'batch-token');
    expect(action.showUploadDialog.value).toBe(true);
  });

  it('closing the native folder picker is cancellation, not an error or a never-ending task', async () => {
    mocks.saveBatch.mockResolvedValueOnce(null);
    const action = useMediaAction(ref('diary'), ref(undefined), ref(false), ref(undefined));
    await action.saveDecryptAttachments(['first']);
    expect(mocks.cancelUnstarted).toHaveBeenCalledExactlyOnceWith('task');
    expect(mocks.fail).not.toHaveBeenCalled(); expect(mocks.register).not.toHaveBeenCalled();
    expect(action.showUploadDialog.value).toBe(false);
  });

  it('does not start empty batches and reports command failures', async () => {
    const action = useMediaAction(ref('diary'), ref(undefined), ref(false), ref(undefined));
    await action.saveDecryptAttachments([]); expect(mocks.saveBatch).not.toHaveBeenCalled();
    mocks.saveBatch.mockRejectedValueOnce(new Error('无法读取目录'));
    await action.saveDecryptAttachments(['first']);
    expect(mocks.fail).toHaveBeenCalledWith('task', expect.objectContaining({message: '无法读取目录'}));
    expect(action.showUploadDialog.value).toBe(true);
  });

  it('uses download progress when saving a decrypted attachment to a local file', async () => {
    const action = useMediaAction(ref('diary'), ref(undefined), ref(false), ref(undefined));
    await action.saveDecryptAttachment('attachment');
    expect(mocks.createTask).toHaveBeenCalledWith('报告.pdf', false, 'download');
    expect(mocks.save).toHaveBeenCalledWith({}, 'diary', 'attachment');
    expect(mocks.register).toHaveBeenCalledWith('task', 'save-token');

    const [filename, , direction] = mocks.createTask.mock.calls[0];
    const task = createUploadTask('task', filename, direction);
    applyUploadTaskEvent(task, {event: 'started'});
    applyUploadTaskEvent(task, {event: 'progress', data: 42});
    expect(uploadTaskStatusText(task)).toBe('下载中 42%');
    applyUploadTaskEvent(task, {event: 'finalizing'});
    expect(uploadTaskStatusText(task)).toBe('即将完成：写入本地文件');
    applyUploadTaskEvent(task, {event: 'completedWithoutData'});
    expect(uploadTaskStatusText(task)).toBe('已完成');
  });

  it('keeps upload direction and backend arguments for encryption and rotation', async () => {
    const action = useMediaAction(ref('diary'), ref(undefined), ref(false), ref(undefined));
    await action.toggleAttachmentEncryption('attachment');
    await action.rotateAttachment('attachment', 90);
    expect(mocks.createTask.mock.calls).toEqual([
      ['报告.pdf', false, 'upload'], ['报告.pdf', false, 'upload'],
    ]);
    expect(mocks.rotate).toHaveBeenCalledWith({}, 'diary', 'attachment', 90);
  });
});
