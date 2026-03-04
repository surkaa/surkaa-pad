import {computed, nextTick, onDeactivated, ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import {useRouter} from 'vue-router';
import {commands, DiarySummary} from "../bindings.ts";
import {EXTENSIONS} from "../components/editor/extension.ts";
import {DiaryChangedEvent} from "../types";
import {useEventBus} from "@vueuse/core";

export function useDiaryCore(initialId: string) {
    const $q = useQuasar();
    const router = useRouter();

    const bus = useEventBus<DiaryChangedEvent>('diary-changed');
    const diaryId = ref<string>(initialId);
    const diary = ref<DiarySummary>();
    const diaryContent = ref<string>("");
    const attachmentMap = ref<Record<string, string>>({});

    const isDelBack = ref(false);

    // 标记是否已经完成初次加载，避免将后端的初次赋值误认为用户的输入
    const isInitialLoaded = ref(false);

    const isNew = computed(() => diaryId.value.trim() === "");

    let saveTimeout: ReturnType<typeof setTimeout> | null = null;
    const AUTO_SAVE_DELAY = 1000;

    const unusedAttachments = computed(() => {
        if (!diary.value) return [];

        return diary.value.attachments.filter(attachment => {
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
        const [summaryRes, contentRes] = await Promise.all([
            commands.cmdGetDiarySummary(diaryId.value),
            commands.cmdGetDiaryContent(diaryId.value)
        ]);

        if (summaryRes.status === 'error' || contentRes.status === 'error') {
            console.error(`加载日记失败:`, summaryRes, contentRes);
            return;
        }

        diary.value = summaryRes.data;
        const [content, map] = contentRes.data;
        diaryContent.value = content;
        attachmentMap.value = map as Record<string, string>;
        // 延迟标记加载完成，避免触发首次 watch
        await nextTick();
        isInitialLoaded.value = true;
    }

    async function saveDiary() {
        if (isNew.value) {
            const res = await commands.cmdSaveDiary(diaryContent.value);
            if (res.status === 'error') {
                $q.notify({type: 'negative', message: `保存日记失败: ${res.error}`});
                return;
            }
            const [summary, content] = res.data;
            diaryId.value = summary.id;
            diary.value = summary;
            diaryContent.value = content;
            $q.notify({type: 'positive', message: '日记已自动创建'});
            bus.emit({type: 'created', summary});
            return;
        }

        // 已存在的日记，执行更新
        const res = await commands.cmdUpdateDiaryContentOnly(diaryId.value, diaryContent.value);
        if (res.status === 'error') {
            $q.notify({type: 'negative', message: `保存日记失败: ${res.error}`});
            return;
        }
        diary.value = res.data;
        bus.emit({type: 'updated', summary: res.data});
    }

    function deleteDiary() {
        if (!diaryId.value) return;
        $q.dialog({
            title: '确认删除',
            message: '确定要删除这篇日记吗？此操作无法撤销。',
            ok: {label: '删除', color: 'negative', flat: true},
            cancel: {label: '取消', color: 'primary', flat: true}
        }).onOk(async () => {
            const res = await commands.cmdDeleteDiary(diaryId.value);
            if (res.status === 'error') {
                $q.notify({type: 'negative', message: `删除日记失败: ${res.error}`});
                return;
            }
            $q.notify({type: 'positive', message: '日记已删除'});
            bus.emit({type: 'deleted', id: diaryId.value});
            isDelBack.value = true;
            router.back();
        });
    }

    // 监听日记内容的变化
    watch(diaryContent, (newValue, oldValue) => {
        // 如果还没加载完，或者值根本没变，则不触发保存
        if (!isInitialLoaded.value || newValue === oldValue) return;
        // 清除上一次的定时器（防抖）
        if (saveTimeout) clearTimeout(saveTimeout);
        // 开启新的定时器
        saveTimeout = setTimeout(saveDiary, AUTO_SAVE_DELAY);
    });

    // 组件卸载时，如果还有没保存的，强制保存一次
    onDeactivated(async () => {
        if (saveTimeout) {
            clearTimeout(saveTimeout);
            await saveDiary();
        }
    });

    return {
        diaryId,
        diary,
        diaryContent,
        attachmentMap,
        isInitialLoaded,
        isNew,
        unusedAttachments,
        isDelBack,
        loadDiaryInfo,
        deleteDiary
    };
}