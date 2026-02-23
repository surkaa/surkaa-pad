import {computed, onDeactivated, ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import {useRouter} from 'vue-router';
import {commands, DiarySummary} from "../bindings.ts";
import {eventBusEmit} from "../utils/eventBus.ts";

export function useDiaryCore(initialId: string) {
    const $q = useQuasar();
    const router = useRouter();

    const diaryId = ref<string>(initialId);
    const diary = ref<DiarySummary>();
    const diaryContent = ref<string>("");

    // 标记是否已经完成初次加载，避免将后端的初次赋值误认为用户的输入
    const isInitialLoaded = ref(false);

    const isNew = computed(() => diaryId.value.trim() === "");

    let saveTimeout: ReturnType<typeof setTimeout> | null = null;
    const AUTO_SAVE_DELAY = 1000;

    const loadDiaryInfo = async () => {
        const [summaryRes, contentRes] = await Promise.all([
            commands.cmdGetDiarySummary(diaryId.value),
            commands.cmdGetDiaryContent(diaryId.value)
        ]);

        if (summaryRes.status === 'error' || contentRes.status === 'error') {
            console.error(`加载日记失败:`, summaryRes, contentRes);
            return;
        }

        diary.value = summaryRes.data;
        diaryContent.value = contentRes.data;
        // 延迟标记加载完成，避免触发首次 watch
        setTimeout(() => {
            isInitialLoaded.value = true;
        }, 50);
    };

    const saveDiary = async () => {
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
            eventBusEmit('diary-changed', {type: 'created', summary});
            return;
        }

        // 已存在的日记，执行更新
        const res = await commands.cmdUpdateDiaryContentOnly(diaryId.value, diaryContent.value);
        if (res.status === 'error') {
            $q.notify({type: 'negative', message: `保存日记失败: ${res.error}`});
            return;
        }
        diary.value = res.data;
        eventBusEmit('diary-changed', {type: 'updated', summary: res.data});
    };

    const deleteDiary = () => {
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
            eventBusEmit('diary-changed', {type: 'deleted', id: diaryId.value});
            router.back();
        });
    };

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
        isInitialLoaded,
        isNew,
        loadDiaryInfo,
        saveDiary,
        deleteDiary
    };
}