import {computed, nextTick, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import {useRouter} from 'vue-router';
import {useDataStore} from "../stores/data.ts";
import {storeToRefs} from "pinia";
import {CloseRequestedEvent, getCurrentWindow} from "@tauri-apps/api/window";
import {UnlistenFn} from "@tauri-apps/api/event";
import api from "../utils/api.ts";
import {
    formatError,
    isNewerDiaryVersionError,
    NEWER_DIARY_VERSION_MESSAGE
} from "../utils/formatError.ts";
import type {DiaryContent, DiaryDraft, DiaryEditAccess} from "../bindings.ts";
import {runDiaryDeletion} from "../utils/diaryDeletion.ts";
import {findUnusedAttachments} from '../utils/diaryAttachments';
import {LatestTaskQueue} from '../utils/latestTaskQueue';

export function useDiaryCore() {
    const $q = useQuasar();
    const router = useRouter();
    const appWindow = getCurrentWindow();
    const dataStore = useDataStore();
    const {
        currentId,
        currentDiary,
        currentDiaryAttachments,
        diarySummaries,
        currentDiaryAttachmentUrlMap,
    } = storeToRefs(dataStore);

    const diaryContent = ref<DiaryContent>({nodes: []});
    const diaryManifestSize = ref<number>();

    const isDelBack = ref(false);

    // 标记是否已经完成初次加载，避免将后端的初次赋值误认为用户的输入
    const isInitialLoaded = ref(false);

    const isNew = computed(() => currentId.value.trim() === "");
    // 不在延迟任务中读取全局 currentId，避免切换日记后把旧正文写进新日记。
    let openedId = currentId.value;
    const access = ref<DiaryEditAccess>({sessionId: null, readOnly: false, canTakeOver: false, reason: null});
    const checkingEdit = ref(false);
    const localDraft = ref<DiaryDraft | null>(null);
    const hasUnresolvedDraft = ref(false);
    const isReadOnly = computed(() => access.value.readOnly || checkingEdit.value || hasUnresolvedDraft.value);
    let heartbeat: ReturnType<typeof setInterval> | null = null;
    let renewing = false;
    let active = false;
    let wasDeactivated = false;
    let releasedToken: string | null = null;
    let closingSession: Promise<void> | null = null;
    let activationEpoch = 0;

    function stopCloudSaving(error: unknown) {
        access.value = {...access.value, readOnly: true, reason: `已停止编辑；未保存内容保留为本机加密草稿。${formatError(error)}`};
    }

    let saveTimeout: ReturnType<typeof setTimeout> | null = null;
    const AUTO_SAVE_DELAY = 1000;

    function cloneContent(content: DiaryContent): DiaryContent {
        return JSON.parse(JSON.stringify(content)) as DiaryContent;
    }

    const unusedAttachments = computed(() => {
        if (!currentDiary.value) return [];
        return findUnusedAttachments(diaryContent.value, currentDiaryAttachments.value);
    });

    async function loadDiaryInfo(takeover = false): Promise<boolean> {
        isInitialLoaded.value = false;
        openedId = currentId.value;
        try {
            const view = await api.cmdOpenDiaryEdit(openedId, takeover);
            const {detail} = view;
            access.value = view.access;
            localDraft.value = view.draft;
            hasUnresolvedDraft.value = view.draft !== null;
            diarySummaries.value[openedId] = detail.summary;
            diaryManifestSize.value = detail.manifestSize;
            diaryContent.value = detail.content;
            currentDiaryAttachments.value = detail.attachments;
            currentDiaryAttachmentUrlMap.value = detail.attachmentUrls as Record<string, string>;
            // 延迟标记加载完成，避免触发首次 watch
            await nextTick();
            isInitialLoaded.value = true;
            return true;
        } catch (e) {
            console.error(`加载日记失败:`, e);
            $q.notify({
                type: isNewerDiaryVersionError(e) ? 'warning' : 'negative',
                message: isNewerDiaryVersionError(e)
                    ? NEWER_DIARY_VERSION_MESSAGE
                    : `加载日记失败: ${formatError(e)}`
            });
            return false;
        }
    }

    interface PendingSave {id: string; content: DiaryContent; sessionId: string | null; revision: string | null}

    const saveQueue = new LatestTaskQueue<PendingSave>(async payload => {
        // 旧任务不得借用重新申请后的会话资格；其草稿仍然保留。
        if (access.value.readOnly || payload.id !== openedId || payload.sessionId !== access.value.sessionId) return;
        try {
            diarySummaries.value[payload.id] = await api.cmdUpdateDiaryContentOnly(
                payload.id, payload.content, payload.sessionId, payload.revision,
            );
            if (localDraft.value?.revision === payload.revision) localDraft.value = null;
        } catch (error) {
            stopCloudSaving(error);
            $q.notify({type: 'warning', message: access.value.reason!});
            // 不重试云端覆盖，允许在草稿已落盘后离开。
        }
    });

    const draftQueue = new LatestTaskQueue<PendingSave>(async payload => {
        if (!payload.id && !openedId) {
            const [summary] = await api.cmdSaveDiary(payload.content);
            openedId = summary.id;
            currentId.value = summary.id;
            dataStore.insertNewDiary(summary);
            try {
                const view = await api.cmdOpenDiaryEdit(openedId, false);
                access.value = view.access;
            } catch (error) { stopCloudSaving(error); }
            $q.notify({type: 'positive', message: '日记已自动创建'});
            return;
        }
        if (!payload.id) payload = {...payload, id: openedId, sessionId: access.value.sessionId};
        const revision = await api.cmdSaveDiaryDraft(payload.id, payload.content);
        localDraft.value = {version: 1, diaryId: payload.id, content: payload.content, revision, savedAt: Date.now()};
        saveQueue.request({...payload, revision});
        scheduleSave();
    });

    function pendingSave(): PendingSave {
        return {id: openedId, content: cloneContent(diaryContent.value), sessionId: access.value.sessionId, revision: null};
    }

    async function ensureDiaryCreated(): Promise<boolean> {
        if (isReadOnly.value) return false;
        if (!isNew.value) return true;
        try {
            draftQueue.request(pendingSave());
            await draftQueue.flush();
            return !isReadOnly.value;
        } catch (error) {
            $q.notify({type: 'negative', message: `创建日记失败: ${formatError(error)}`});
            return false;
        }
    }

    async function flushPendingSave(): Promise<boolean> {
        if (saveTimeout) {
            clearTimeout(saveTimeout);
            saveTimeout = null;
        }
        try {
            await draftQueue.flush();
            await saveQueue.flush();
            return true;
        } catch (e) {
            $q.notify({type: 'negative', message: `保存本机草稿失败，请不要关闭页面：${formatError(e)}`});
            return false;
        }
    }

    function scheduleSave() {
        if (!active) return;
        if (saveTimeout) clearTimeout(saveTimeout);
        saveTimeout = setTimeout(() => {
            saveTimeout = null;
            void flushPendingSave();
        }, AUTO_SAVE_DELAY);
    }

    function deleteDiary() {
        if (!currentId.value || isReadOnly.value) return;
        const id = openedId;
        const sessionId = access.value.sessionId;
        $q.dialog({
            title: '确认删除',
            message: '确定要删除这篇日记吗？此操作无法撤销。',
            ok: {label: '删除', color: 'negative', flat: true},
            cancel: {label: '取消', color: 'primary', flat: true}
        }).onOk(async () => {
            if (!(await flushPendingSave())) return;
            await runDiaryDeletion(
                () => api.cmdDeleteDiary(id, sessionId),
                () => {
                    $q.notify({type: 'positive', message: '日记已删除'});
                    dataStore.deleteSummary(id);
                    isDelBack.value = true;
                    router.back();
                },
                (error) => {
                    $q.notify({type: 'negative', message: `删除日记失败: ${formatError(error)}`});
                },
            );
        });
    }

    function updateContent(newContent: DiaryContent) {
        if (isReadOnly.value) return;
        if (JSON.stringify(diaryContent.value) === JSON.stringify(newContent)) return;
        diaryContent.value = newContent;
    }

    // 监听日记内容的变化
    watch(diaryContent, (newValue, oldValue) => {
        // 如果还没加载完，或者值根本没变，则不触发保存
        if (!isInitialLoaded.value || access.value.readOnly || newValue === oldValue) return;
        draftQueue.request(pendingSave());
        // 草稿立即排队落盘，云端保存仍做 1 秒合并，且必须在草稿落盘之后。
        void draftQueue.flush().catch(error => {
            $q.notify({type: 'negative', message: `本机草稿保存失败，请不要关闭页面：${formatError(error)}`});
        });
    });

    async function requestEditing() {
        if (checkingEdit.value || !(await flushPendingSave())) return;
        const takeover = access.value.canTakeOver;
        if (takeover) {
            const confirmed = await new Promise<boolean>(resolve => $q.dialog({
                title: '接管编辑', message: '上次占用已过期。请确认其他设备已停止编辑；接管后将重新加载云端日记，本机草稿仍保留。',
                cancel: {flat: true, label: '取消'}, ok: {label: '接管编辑', color: 'primary'},
            }).onOk(() => resolve(true)).onCancel(() => resolve(false)));
            if (!confirmed) return;
        }
        checkingEdit.value = true;
        try { await loadDiaryInfo(takeover); } finally { checkingEdit.value = false; }
    }

    function restoreDraft() {
        if (!localDraft.value || access.value.readOnly || checkingEdit.value) return;
        const draft = localDraft.value;
        $q.dialog({title: '恢复本机草稿', message: '将本机草稿恢复到编辑器并自动保存到云端，是否继续？',
            cancel: {flat: true, label: '取消'}, ok: {label: '恢复', color: 'primary'},
        }).onOk(() => {
            if (access.value.readOnly || checkingEdit.value || localDraft.value?.revision !== draft.revision) return;
            hasUnresolvedDraft.value = false;
            diaryContent.value = cloneContent(draft.content);
        });
    }

    function discardDraft() {
        const draft = localDraft.value;
        if (!draft) return;
        $q.dialog({title: '放弃本机草稿', message: '只删除这份本机草稿，不修改云端日记。此操作无法撤销。',
            cancel: {flat: true, label: '取消'}, ok: {label: '放弃草稿', color: 'negative'},
        }).onOk(async () => {
            try {
                await draftQueue.flush();
                if (await api.cmdDiscardDiaryDraft(draft.diaryId, draft.revision)) {
                    localDraft.value = null;
                    hasUnresolvedDraft.value = false;
                }
            } catch (error) { $q.notify({type: 'negative', message: `放弃草稿失败：${formatError(error)}`}); }
        });
    }

    async function renewEdit() {
        const id = openedId;
        const token = access.value.sessionId;
        if (!token || access.value.readOnly || renewing || document.visibilityState === 'hidden') return;
        renewing = true;
        try { await api.cmdRenewDiaryEdit(id, token); }
        catch (error) {
            if (openedId === id && access.value.sessionId === token) stopCloudSaving(error);
        } finally { renewing = false; }
    }
    const onVisibilityChange = async () => {
        if (document.visibilityState === 'hidden' || access.value.readOnly) return;
        checkingEdit.value = true;
        try { await renewEdit(); } finally { checkingEdit.value = false; }
    };

    async function releaseEdit(id: string, token: string | null) {
        if (!token || releasedToken === token) return;
        releasedToken = token;
        try { await api.cmdReleaseDiaryEdit(id, token); }
        catch (error) { console.warn('释放日记编辑占用失败，将等待占用过期', error); }
    }

    let unlisten: UnlistenFn | null = null;
    // 关闭窗口的处理逻辑
    const handleWindowClose = async (event: CloseRequestedEvent) => {
        event.preventDefault();
        const id = openedId;
        const token = access.value.sessionId;
        if (await flushPendingSave()) {
            await releaseEdit(id, token);
            await appWindow.destroy();
        }
    };
    async function activate() {
        if (active) return;
        active = true;
        const epoch = ++activationEpoch;
        await closingSession;
        if (wasDeactivated && openedId) await loadDiaryInfo();
        if (!active || epoch !== activationEpoch) return;
        // 防止重复注册
        if (!unlisten) {
            const listener = await appWindow.onCloseRequested(handleWindowClose);
            if (active && epoch === activationEpoch) unlisten = listener; else listener();
        }
        if (active) {
            if (!heartbeat) heartbeat = setInterval(() => void renewEdit(), 30_000);
            document.addEventListener('visibilitychange', onVisibilityChange);
        }
    }
    onMounted(activate);
    onActivated(activate);

    function cleanupListeners() {
        active = false;
        activationEpoch += 1;
        wasDeactivated = true;
        if (heartbeat) clearInterval(heartbeat);
        heartbeat = null;
        document.removeEventListener('visibilitychange', onVisibilityChange);
        if (saveTimeout) clearTimeout(saveTimeout);
        saveTimeout = null;
    }

    function closeSession() {
        if (closingSession) return closingSession;
        const id = openedId;
        const token = access.value.sessionId;
        closingSession = flushPendingSave().then(saved => saved ? releaseEdit(id, token) : undefined)
            .finally(() => { closingSession = null; });
        return closingSession;
    }

    onUnmounted(() => {
        cleanupListeners();
        if (unlisten) {
            unlisten();
            unlisten = null;
        }
        void closeSession();
    });

    // 组件卸载时，如果还有没保存的，强制保存一次
    onDeactivated(async () => {
        cleanupListeners();
        // 卸载窗口关闭监听器，防止后台堆积
        if (unlisten) {
            unlisten();
            unlisten = null;
        }

        await closeSession();
    });

    return {
        diaryId: currentId,
        diary: currentDiary,
        attachments: currentDiaryAttachments,
        diaryManifestSize,
        diaryContent,
        attachmentMap: currentDiaryAttachmentUrlMap,
        isInitialLoaded,
        isNew,
        unusedAttachments,
        isDelBack,
        isReadOnly, access, checkingEdit, localDraft, hasUnresolvedDraft, requestEditing, restoreDraft, discardDraft,
        loadDiaryInfo,
        deleteDiary,
        updateContent,
        ensureDiaryCreated,
        flushPendingSave,
    };
}
