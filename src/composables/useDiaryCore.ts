import {computed, nextTick, onActivated, onDeactivated, onUnmounted, ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import {useRouter} from 'vue-router';
import {EXTENSIONS} from "../components/editor/extension.ts";
import {useDataStore} from "../stores/data.ts";
import {storeToRefs} from "pinia";
import {CloseRequestedEvent, getCurrentWindow} from "@tauri-apps/api/window";
import {UnlistenFn} from "@tauri-apps/api/event";
import api from "../utils/api.ts";
import {formatError} from "../utils/formatError.ts";

export function useDiaryCore() {
    const $q = useQuasar();
    const router = useRouter();
    const appWindow = getCurrentWindow();
    const dataStore = useDataStore();
    const {currentId, currentDiary, diarySummaries, currentDiaryAttachmentUrlMap} = storeToRefs(dataStore);

    const diaryContent = ref<string>("");

    const isDelBack = ref(false);

    // 标记是否已经完成初次加载，避免将后端的初次赋值误认为用户的输入
    const isInitialLoaded = ref(false);

    const isNew = computed(() => currentId.value.trim() === "");

    let saveTimeout: ReturnType<typeof setTimeout> | null = null;
    const AUTO_SAVE_DELAY = 1000;

    const unusedAttachments = computed(() => {
        if (!currentDiary.value) return [];

        return currentDiary.value.attachments.filter(attachment => {
            let isReferenced = false;
            for (const ext of EXTENSIONS) {
                // 使用之前修复的正则校验
                if (ext.hasMark && ext.hasMark(diaryContent.value, attachment.filename)) {
                    isReferenced = true;
                    break;
                } else if (ext.getMark) {
                    const mark = ext.getMark(attachment.filename);
                    if (diaryContent.value.includes(mark)) {
                        isReferenced = true;
                        break;
                    }
                }
            }
            return !isReferenced;
        });
    });

    async function loadDiaryInfo() {
        try {
            const [summaryRes, contentRes] = await Promise.all([
                api.cmdGetDiarySummary(currentId.value),
                api.cmdGetDiaryContent(currentId.value)
            ]);

            diarySummaries.value[currentId.value] = summaryRes;
            const [content, map] = contentRes;
            diaryContent.value = content;
            currentDiaryAttachmentUrlMap.value = map as Record<string, string>;
            // 延迟标记加载完成，避免触发首次 watch
            await nextTick();
            isInitialLoaded.value = true;
        } catch (e) {
            console.error(`加载日记失败:`, e);
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
            try {
                await api.cmdDeleteDiary(currentId.value);
            } catch (e) {
                $q.notify({type: 'negative', message: `删除日记失败: ${formatError(e)}`});
            }
            $q.notify({type: 'positive', message: '日记已删除'});
            dataStore.deleteSummary(currentId.value);
            isDelBack.value = true;
            router.back();
        });
    }

    function updateContent(newContent: string) {
        if (diaryContent.value === newContent) return;
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