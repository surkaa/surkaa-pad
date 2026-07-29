import {computed, nextTick, onActivated, onDeactivated, onUnmounted, ref, watch} from 'vue';
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
import type {DiaryContent} from "../bindings.ts";
import {runDiaryDeletion} from "../utils/diaryDeletion.ts";
import {findUnusedAttachments} from '../utils/diaryAttachments';

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

    let saveTimeout: ReturnType<typeof setTimeout> | null = null;
    const AUTO_SAVE_DELAY = 1000;

    const unusedAttachments = computed(() => {
        if (!currentDiary.value) return [];
        return findUnusedAttachments(diaryContent.value, currentDiaryAttachments.value);
    });

    async function loadDiaryInfo() {
        try {
            const detail = await api.cmdGetDiaryDetail(currentId.value);
            diarySummaries.value[currentId.value] = detail.summary;
            diaryManifestSize.value = detail.manifestSize;
            diaryContent.value = detail.content;
            currentDiaryAttachments.value = detail.attachments;
            currentDiaryAttachmentUrlMap.value = detail.attachmentUrls as Record<string, string>;
            // 延迟标记加载完成，避免触发首次 watch
            await nextTick();
            isInitialLoaded.value = true;
        } catch (e) {
            console.error(`加载日记失败:`, e);
            $q.notify({
                type: isNewerDiaryVersionError(e) ? 'warning' : 'negative',
                message: isNewerDiaryVersionError(e)
                    ? NEWER_DIARY_VERSION_MESSAGE
                    : `加载日记失败: ${formatError(e)}`
            });
        }
    }

    async function saveDiary() {
        if (isNew.value) {
            try {
                const [summary, content] = await api.cmdSaveDiary(diaryContent.value);
                currentId.value = summary.id;
                dataStore.insertNewDiary(summary);
                diaryContent.value = content;
                $q.notify({type: 'positive', message: '日记已自动创建'});
            } catch (e) {
                $q.notify({type: 'negative', message: `保存日记失败: ${formatError(e)}`});
            }
            return;
        }

        try {
            // 已存在的日记，执行更新
            diarySummaries.value[currentId.value] = await api.cmdUpdateDiaryContentOnly(
                currentId.value,
                diaryContent.value
            );
        } catch (e) {
            $q.notify({type: 'negative', message: `保存日记失败: ${formatError(e)}`});
        }
    }

    function deleteDiary() {
        if (!currentId.value) return;
        $q.dialog({
            title: '确认删除',
            message: '确定要删除这篇日记吗？此操作无法撤销。',
            ok: {label: '删除', color: 'negative', flat: true},
            cancel: {label: '取消', color: 'primary', flat: true}
        }).onOk(async () => {
            await runDiaryDeletion(
                () => api.cmdDeleteDiary(currentId.value),
                () => {
                    $q.notify({type: 'positive', message: '日记已删除'});
                    dataStore.deleteSummary(currentId.value);
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
        if (JSON.stringify(diaryContent.value) === JSON.stringify(newContent)) return;
        diaryContent.value = newContent;
    }

    // 监听日记内容的变化
    watch(diaryContent, (newValue, oldValue) => {
        // 如果还没加载完，或者值根本没变，则不触发保存
        if (!isInitialLoaded.value || newValue === oldValue) return;
        // 清除上一次的定时器（防抖）
        if (saveTimeout) clearTimeout(saveTimeout);
        // 开启新的定时器
        saveTimeout = setTimeout(async () => {
            await saveDiary();
            saveTimeout = null;
        }, AUTO_SAVE_DELAY);
    });

    let unlisten: UnlistenFn | null = null;
    // 关闭窗口的处理逻辑
    const handleWindowClose = async (event: CloseRequestedEvent) => {
        event.preventDefault();
        try {
            if (saveTimeout) {
                clearTimeout(saveTimeout);
                saveTimeout = null;
                await saveDiary();
            }
        } finally {
            await appWindow.destroy();
        }
    };
    onActivated(async () => {
        // 防止重复注册
        if (!unlisten) {
            unlisten = await appWindow.onCloseRequested(handleWindowClose);
        }
    });

    onUnmounted(() => {
        if (unlisten) {
            unlisten();
            unlisten = null;
        }
    });

    // 组件卸载时，如果还有没保存的，强制保存一次
    onDeactivated(async () => {
        // 卸载窗口关闭监听器，防止后台堆积
        if (unlisten) {
            unlisten();
            unlisten = null;
        }

        // 处理未保存的内容
        if (saveTimeout) {
            clearTimeout(saveTimeout);
            saveTimeout = null;
            await saveDiary();
        }
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
        loadDiaryInfo,
        deleteDiary,
        updateContent
    };
}
