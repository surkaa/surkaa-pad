// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, reactive, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import Component from '../UploadTasksDialog.vue';
import {createUploadTask, type UploadTask} from '../../../utils/uploadTasks';

const mocks = vi.hoisted(() => ({open: vi.fn(), notify: vi.fn()}));
vi.mock('../../../utils/api', () => ({default: {cmdOpenAttachmentDownloadLocation: mocks.open}}));
vi.mock('quasar', async importOriginal => ({
  ...await importOriginal<typeof import('quasar')>(), useQuasar: () => ({notify: mocks.notify}),
}));
let app: App | undefined;
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(350); await nextTick(); }
function saved(id = 'saved'): UploadTask {
  return {...createUploadTask(id, 'report.pdf', 'download'), status: 'completed', progress: 1, downloadLocationToken: `${id}-location`};
}
async function mount(tasks: UploadTask[], dark: boolean, allSettled = true) {
  const props = reactive({modelValue: true, tasks, allSettled});
  const host = document.createElement('div'); document.body.append(host);
  app = createApp({render: () => h(Component, props)});
  app.use(Quasar); Dark.set(dark); app.mount(host); await settle();
  return props;
}
function openButton() {
  return [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.includes('打开下载位置'));
}
beforeEach(() => { vi.useFakeTimers(); vi.clearAllMocks(); mocks.open.mockResolvedValue(undefined); });
afterEach(() => { app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers(); });

describe.each([false, true])('下载完成弹窗（深色=%s）', dark => {
  it('成功下载显示打开位置按钮，只传句柄，不关闭结果弹窗', async () => {
    await mount([saved()], dark);
    expect(openButton()).toBeDefined();
    expect(openButton()!.classList.contains('text-primary')).toBe(true);
    openButton()!.click(); await settle();
    expect(mocks.open).toHaveBeenCalledExactlyOnceWith('saved-location');
    expect(document.querySelector('.q-dialog')).not.toBeNull();
    expect(mocks.notify).not.toHaveBeenCalled();
  });

  it('进行中、失败、取消、缓存和上传都不提供打开位置', async () => {
    const props = await mount([saved()], dark, false);
    expect(openButton()).toBeUndefined();
    props.allSettled = true;
    for (const status of ['error', 'canceled', 'uploading'] as const) {
      props.tasks = [{...saved(), status}]; await settle(); expect(openButton()).toBeUndefined();
    }
    props.tasks = [{...saved(), direction: 'upload'}]; await settle(); expect(openButton()).toBeUndefined();
    props.tasks = [{...saved(), downloadLocationToken: undefined}]; await settle(); expect(openButton()).toBeUndefined();
  });

  it('避免重复打开，系统打开失败时展示原因并允许重试，不改变下载成功状态', async () => {
    let reject!: (reason: Error) => void;
    mocks.open.mockImplementationOnce(() => new Promise<void>((_, no) => {reject = no;}));
    const props = await mount([saved()], dark);
    openButton()!.click(); openButton()!.click(); await nextTick();
    expect(mocks.open).toHaveBeenCalledTimes(1);
    reject(new Error('保存目录已被移动')); await settle();
    expect(mocks.notify).toHaveBeenCalledWith(expect.objectContaining({message: expect.stringContaining('保存目录已被移动')}));
    expect(props.tasks[0].status).toBe('completed'); expect(openButton()!.disabled).toBe(false);
    openButton()!.click(); await settle(); expect(mocks.open).toHaveBeenCalledTimes(2);
  });
});
