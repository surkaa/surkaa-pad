// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Dark, Quasar, Notify} from 'quasar';
import type {ArchiveEntryEvent, ArchiveEntryTarget} from '../../bindings';
import {buildArchiveTree} from '../../utils/archivePreview';
import ArchiveEntryPreview from '../ArchiveEntryPreview.vue';

const mocks = vi.hoisted(() => ({prepare: vi.fn(), cancel: vi.fn(), release: vi.fn(), open: vi.fn()}));
vi.mock('../../utils/api', () => ({default: {cmdPrepareArchiveEntry: mocks.prepare, cmdCancelTask: mocks.cancel,
  cmdReleaseArchiveEntry: mocks.release, cmdOpenArchiveEntryHtml: mocks.open}}));
vi.mock('@tauri-apps/api/core', () => ({Channel: class {onmessage = (_message: unknown) => undefined;}}));
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl: vi.fn()}));
let app: App | undefined;
let channel: {onmessage: (message: ArchiveEntryEvent) => void};
const entry = buildArchiveTree([{path: 'file.json', size: 7, compressedSize: 7, isDirectory: false, encrypted: false, modifiedAt: null}])[0]!;
async function mount(extra: Record<string, unknown> = {}) {
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(ArchiveEntryPreview, {diaryId: 'diary', attachmentId: 'archive', entry, revision: 'v1', ...extra});
  app.use(Quasar, {plugins: {Notify}}).mount(host); await nextTick();
}
beforeEach(() => {
  vi.clearAllMocks();
  mocks.cancel.mockResolvedValue(undefined); mocks.release.mockResolvedValue(undefined);
  mocks.prepare.mockImplementation(async (_target: ArchiveEntryTarget, _password: string | null, event: typeof channel) => {channel = event; return 'task';});
});
afterEach(() => {app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = ''; vi.unstubAllGlobals();});

describe('archive entry loading', () => {
  it.each([false, true])('shows real byte progress and separate network reads (dark=%s)', async dark => {
    Dark.set(dark); await mount({solid: true});
    expect(document.body.textContent).toContain('正在准备读取');
    expect(document.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBeNull();
    const waitingModel = document.querySelector('.q-linear-progress__model');
    channel.onmessage({event: 'reading', data: {bytes: 1024 * 1024}});
    channel.onmessage({event: 'progress', data: {progress: {phase: 'extracting', processedBytes: 50, totalBytes: 100}}});
    await nextTick();
    expect(document.body.textContent).toContain('50%');
    const realModel = document.querySelector<HTMLElement>('.q-linear-progress__model')!;
    expect(realModel).not.toBe(waitingModel);
    expect(realModel.style.transform).toBe('scale3d(0.5,1,1)');
    channel.onmessage({event: 'progress', data: {progress: {phase: 'extracting', processedBytes: 75, totalBytes: 100}}});
    await nextTick();
    expect(document.body.textContent).toContain('75%');
    expect(document.querySelector('.q-linear-progress__model')).toBe(realModel);
    expect(realModel.style.transform).toBe('scale3d(0.75,1,1)');
    expect(document.body.textContent).toContain('已读取压缩数据 1 MB');
    expect(document.body.textContent).toContain('固实压缩');
    channel.onmessage({event: 'progress', data: {progress: {phase: 'verifying', processedBytes: 0, totalBytes: 0}}});
    await nextTick(); expect(document.body.textContent).toContain('正在校验');
  });

  it('cancels pending tasks and discards late completed resources after unmount', async () => {
    await mount(); app!.unmount(); app = undefined;
    expect(mocks.cancel).toHaveBeenCalledWith('task');
    channel.onmessage({event: 'completed', data: {resource: {id: 'late', filename: 'file.json', size: 7, mimetype: 'application/json', url: 'http://localhost/archive-entry/late'}}});
    expect(mocks.release).toHaveBeenCalledWith('late', true);
  });

  it('cancels the token even when the command reply arrives after leaving', async () => {
    let finish!: (token: string) => void;
    mocks.prepare.mockImplementation((_target, _password, event) => {channel = event; return new Promise<string>(resolve => {finish = resolve;});});
    await mount(); app!.unmount(); app = undefined; finish('late-token'); await nextTick();
    expect(mocks.cancel).toHaveBeenCalledWith('late-token');
  });

  it('prompts again for encrypted content and sends the password only to the preparation command', async () => {
    await mount(); channel.onmessage({event: 'passwordRequired', data: {invalidPassword: true}}); await nextTick();
    expect(document.body.textContent).toContain('密码不正确');
    const input = document.querySelector<HTMLInputElement>('input[type="password"]')!;
    input.value = 'correct'; input.dispatchEvent(new Event('input', {bubbles: true})); await nextTick();
    input.dispatchEvent(new KeyboardEvent('keyup', {key: 'Enter', bubbles: true})); await nextTick();
    expect(mocks.prepare.mock.calls[1]?.[1]).toBe('correct');
    expect(document.querySelector('input[type="password"]')).toBeNull();
  });

  it('does not start a task for unsupported or oversized entries', async () => {
    await mount({entry: {...entry, label: 'inner.7z', path: 'inner.7z'}});
    expect(mocks.prepare).not.toHaveBeenCalled(); expect(document.body.textContent).toContain('嵌套压缩包');
    app!.unmount(); app = undefined;
    await mount({entry: {...entry, size: 256 * 1024 ** 2 + 1}});
    expect(mocks.prepare).not.toHaveBeenCalled(); expect(document.body.textContent).toContain('256 MiB');
  });

  it('shows JSON using the existing tree and leaves successful resources cached on return', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"v":1}')));
    const prepared = vi.fn(); await mount({onPrepared: prepared});
    channel.onmessage({event: 'completed', data: {resource: {id: 'one', filename: 'file.json', size: 7, mimetype: 'application/json', url: 'http://localhost/archive-entry/one'}}});
    await new Promise(resolve => setTimeout(resolve, 10)); await nextTick();
    expect(document.querySelector('.json-tree-viewer')).not.toBeNull();
    expect(prepared).toHaveBeenCalledWith('one');
    app!.unmount(); app = undefined;
    expect(mocks.release).toHaveBeenCalledWith('one', false);
    expect(mocks.cancel).not.toHaveBeenCalled();
  });
});
