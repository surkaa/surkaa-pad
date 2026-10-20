// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, ref, type App} from 'vue';
import type {DiaryContent, DiaryEditView} from '../../bindings';
import {useDiaryCore} from '../useDiaryCore';

const mocks = vi.hoisted(() => ({
  store: {} as Record<string, unknown>,
  notify: vi.fn(), dialog: vi.fn(), confirm: undefined as (() => void) | undefined,
  close: vi.fn(), destroy: vi.fn(), unlisten: vi.fn(),
  open: vi.fn(), draft: vi.fn(), update: vi.fn(), renew: vi.fn(), release: vi.fn(), discard: vi.fn(), create: vi.fn(),
}));
vi.mock('pinia', () => ({storeToRefs: (store: unknown) => store}));
vi.mock('../../stores/data', () => ({useDataStore: () => mocks.store}));
vi.mock('vue-router', () => ({useRouter: () => ({back: vi.fn()})}));
vi.mock('quasar', () => ({useQuasar: () => ({notify: mocks.notify, dialog: mocks.dialog})}));
vi.mock('@tauri-apps/api/window', () => ({getCurrentWindow: () => ({onCloseRequested: mocks.close, destroy: mocks.destroy})}));
vi.mock('../../utils/api', () => ({default: {
  cmdOpenDiaryEdit: mocks.open, cmdSaveDiaryDraft: mocks.draft, cmdUpdateDiaryContentOnly: mocks.update,
  cmdRenewDiaryEdit: mocks.renew, cmdReleaseDiaryEdit: mocks.release, cmdDiscardDiaryDraft: mocks.discard,
  cmdSaveDiary: mocks.create,
}}));

let app: App | undefined;
let core: ReturnType<typeof useDiaryCore>;
const content = (text: string): DiaryContent => ({nodes: [{type: 'markdown', text}]});
const view = (readonly = false, hasDraft = false): DiaryEditView => ({
  access: {sessionId: readonly ? null : 'session-one', readOnly: readonly, canTakeOver: readonly, reason: readonly ? '其他设备正在编辑' : null},
  detail: {summary: {id: '123', title: 'original', created: 1, updated: 1, attachmentCount: 0} as DiaryEditView['detail']['summary'],
    content: content('original'), attachments: [], attachmentUrls: {}, manifestSize: 100},
  draft: hasDraft ? {version: 1, diaryId: '123', revision: 'old-revision', savedAt: 1, content: content('recovery')} : null,
});
async function settle() { for (let i = 0; i < 12; i++) { await Promise.resolve(); await nextTick(); } }
async function mount(result = view()) {
  mocks.open.mockResolvedValue(result);
  app = createApp({setup() { core = useDiaryCore(); return () => h('div'); }});
  app.mount(document.createElement('div'));
  await core.loadDiaryInfo();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  mocks.store = {currentId: ref('123'), currentDiary: ref({id: '123'}), currentDiaryAttachments: ref([]),
    diarySummaries: ref({}), currentDiaryAttachmentUrlMap: ref({}), insertNewDiary: vi.fn(), deleteSummary: vi.fn()};
  mocks.close.mockResolvedValue(mocks.unlisten);
  mocks.draft.mockResolvedValue('draft-revision');
  mocks.update.mockResolvedValue(view().detail.summary);
  mocks.renew.mockResolvedValue(undefined);
  mocks.release.mockResolvedValue(undefined);
  mocks.discard.mockResolvedValue(true);
  mocks.dialog.mockImplementation(() => {
    const chain = {onOk: (fn: () => void) => { mocks.confirm = fn; return chain; }, onCancel: () => chain};
    return chain;
  });
});
afterEach(async () => {
  app?.unmount(); app = undefined;
  await settle();
  vi.clearAllTimers(); vi.useRealTimers();
});

describe('日记编辑占用与草稿保护', () => {
  it('只读打开不排队保存、不更新日记', async () => {
    await mount(view(true));
    expect(core.isReadOnly.value).toBe(true);
    core.updateContent(content('blocked'));
    await core.flushPendingSave();
    expect(core.diaryContent.value).toEqual(content('original'));
    expect(mocks.draft).not.toHaveBeenCalled();
    expect(mocks.update).not.toHaveBeenCalled();
  });

  it('先保存本机草稿，再携带打开时的编辑令牌进行云端保存', async () => {
    await mount();
    core.updateContent(content('changed'));
    await settle();
    expect(mocks.draft).toHaveBeenCalledWith('123', content('changed'));
    expect(mocks.update).not.toHaveBeenCalled();
    await core.flushPendingSave();
    expect(mocks.update).toHaveBeenCalledWith('123', content('changed'), 'session-one', 'draft-revision');
    expect(core.localDraft.value).toBeNull();
  });

  it('草稿写入失败时阻止云端保存与离开，原内容仍在内存中', async () => {
    mocks.draft.mockRejectedValue(new Error('disk full'));
    await mount();
    core.updateContent(content('must not disappear'));
    await settle();
    expect(await core.flushPendingSave()).toBe(false);
    expect(mocks.update).not.toHaveBeenCalled();
    expect(core.diaryContent.value).toEqual(content('must not disappear'));
  });

  it('旧云端回执不能清除保存期间产生的新草稿', async () => {
    await mount();
    let finish!: () => void;
    const wait = new Promise<void>(resolve => { finish = resolve; });
    mocks.update.mockImplementationOnce(async () => { await wait; return view().detail.summary; });
    mocks.draft.mockResolvedValueOnce('revision-one').mockResolvedValueOnce('revision-two');
    core.updateContent(content('first'));
    await settle();
    const saving = core.flushPendingSave();
    await settle();
    core.updateContent(content('second'));
    await settle();
    expect(core.localDraft.value?.revision).toBe('revision-two');
    mocks.update.mockRejectedValueOnce(new Error('network failed for newer content'));
    finish(); await saving;
    expect(core.localDraft.value?.revision).toBe('revision-two');
    expect(core.localDraft.value?.content).toEqual(content('second'));
    expect(core.isReadOnly.value).toBe(true);
  });

  it('接管必须由用户确认后发起，并重新加载云端与草稿', async () => {
    await mount(view(true));
    const takingOver = core.requestEditing();
    await settle();
    expect(mocks.open).toHaveBeenCalledTimes(1);
    mocks.open.mockResolvedValueOnce(view(false, true));
    mocks.confirm?.(); await takingOver;
    expect(mocks.open).toHaveBeenLastCalledWith('123', true);
    expect(core.hasUnresolvedDraft.value).toBe(true);
    expect(core.diaryContent.value).toEqual(content('original'));
    expect(mocks.update).not.toHaveBeenCalled();
  });

  it('云端保存失败转只读、保留草稿，后续 flush 不反复覆盖云端', async () => {
    mocks.update.mockRejectedValue(new Error('lease lost'));
    await mount();
    core.updateContent(content('local only'));
    await settle();
    expect(await core.flushPendingSave()).toBe(true);
    expect(core.isReadOnly.value).toBe(true);
    expect(core.localDraft.value?.content).toEqual(content('local only'));
    await core.flushPendingSave();
    expect(mocks.update).toHaveBeenCalledTimes(1);
    expect(mocks.discard).not.toHaveBeenCalled();
  });

  it('保存排队后切换全局日记 ID，不会把旧内容写入另一篇日记', async () => {
    await mount();
    core.updateContent(content('belongs to 123'));
    await settle();
    (mocks.store.currentId as ReturnType<typeof ref<string>>).value = '456';
    await core.flushPendingSave();
    expect(mocks.update.mock.calls[0][0]).toBe('123');
    expect(mocks.draft.mock.calls[0][0]).toBe('123');
  });

  it('发现草稿时先阻止编辑，用户确认后才恢复并保存', async () => {
    await mount(view(false, true));
    expect(core.isReadOnly.value).toBe(true);
    expect(core.diaryContent.value).toEqual(content('original'));
    expect(mocks.update).not.toHaveBeenCalled();
    core.restoreDraft();
    mocks.confirm?.();
    await settle();
    expect(core.hasUnresolvedDraft.value).toBe(false);
    expect(core.diaryContent.value).toEqual(content('recovery'));
    await core.flushPendingSave();
    expect(mocks.update).toHaveBeenCalledWith('123', content('recovery'), 'session-one', 'draft-revision');
  });

  it('放弃草稿只删除对应本机版本，不写云端正文', async () => {
    await mount(view(false, true));
    core.discardDraft(); mocks.confirm?.();
    await settle();
    expect(mocks.discard).toHaveBeenCalledWith('123', 'old-revision');
    expect(mocks.update).not.toHaveBeenCalled();
    expect(core.isReadOnly.value).toBe(false);
  });

  it('续期失败立即转只读，并且不自动重新申请占用', async () => {
    await mount();
    mocks.renew.mockRejectedValue(new Error('offline'));
    await vi.advanceTimersByTimeAsync(30_000);
    expect(core.isReadOnly.value).toBe(true);
    expect(mocks.open).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(mocks.renew).toHaveBeenCalledTimes(1);
  });

  it('主动退出前先完成保存，再释放自己的编辑令牌', async () => {
    await mount();
    core.updateContent(content('last words'));
    await settle();
    const closeHandler = mocks.close.mock.calls[0][0];
    await closeHandler({preventDefault: vi.fn()});
    expect(mocks.update).toHaveBeenCalledTimes(1);
    expect(mocks.release).toHaveBeenCalledWith('123', 'session-one');
    expect(mocks.destroy).toHaveBeenCalledTimes(1);
    expect(mocks.update.mock.invocationCallOrder[0]).toBeLessThan(mocks.release.mock.invocationCallOrder[0]);
  });
});
