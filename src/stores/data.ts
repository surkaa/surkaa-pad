import {defineStore} from "pinia";
import {computed, ref} from "vue";
import {AttachmentMeta, DiarySummary} from "../bindings.ts";
import {useConfigStore} from "./config.ts";

export const useDataStore = defineStore('data', () => {
    const diaryIds = ref<string[]>([]);
    const diarySummaries = ref<Record<string, DiarySummary | null>>({});
    const diaryListRevision = ref(0);
    const pinnedDiaryIds = useConfigStore().useTauriConfig('pinned_diary_ids');

    // 当前正在编辑的日记ID，空字符串表示新建
    const currentId = ref<string>("");
    const currentDiaryAttachmentUrlMap = ref<Record<string, string>>({});

    const currentDiary = computed(() => diarySummaries.value[currentId.value] || undefined);
    const withAttachments = computed(() => {
        return diarySummaries.value
            ? Object.values(diarySummaries.value).filter(s => s && s.attachments.length).length
            : 0;
    });

    function insertNewDiary(summary: DiarySummary) {
        diarySummaries.value[summary.id] = summary;
        if (!diaryIds.value.includes(summary.id)) {
            // 在头部新增id
            diaryIds.value.unshift(summary.id);
        }
    }

    function deleteSummary(diaryId: string) {
        delete diarySummaries.value[diaryId];
        const index = diaryIds.value.indexOf(diaryId);
        if (index !== -1) {
            diaryIds.value.splice(index, 1);
        }
        // 尝试从pinnedDiaryIds中删除
        const pinnedIndex = pinnedDiaryIds.value.indexOf(diaryId);
        if (pinnedIndex !== -1) {
            pinnedDiaryIds.value = [...pinnedDiaryIds.value.slice(0, pinnedIndex), ...pinnedDiaryIds.value.slice(pinnedIndex + 1)];
        }
    }

    function updateAttachment(diaryId: string, newMeta: AttachmentMeta) {
        const summary = diarySummaries.value[diaryId];
        if (summary) {
            const attachmentIndex = summary.attachments.findIndex(att => att.id === newMeta.id);
            if (attachmentIndex !== -1) {
                summary.attachments[attachmentIndex] = newMeta;
            } else {
                summary.attachments.push(newMeta);
            }
        }
    }

    function updateAttachmentFilename(diaryId: string, attachmentId: string, newFilename: string) {
        const summary = diarySummaries.value[diaryId];
        if (summary) {
            const attachmentIndex = summary.attachments.findIndex(att => att.id === attachmentId);
            if (attachmentIndex !== -1) {
                summary.attachments[attachmentIndex].filename = newFilename;
            }
        }
    }

    function deleteAttachment(diaryId: string, attachmentIds: string[]) {
        const summary = diarySummaries.value[diaryId];
        if (summary) {
            summary.attachments = summary.attachments.filter(att => !attachmentIds.includes(att.id));
        }
    }

    function invalidateDiaryList() {
        diaryIds.value = [];
        diarySummaries.value = {};
        currentId.value = '';
        currentDiaryAttachmentUrlMap.value = {};
        diaryListRevision.value += 1;
    }

    return {
        diaryIds,
        diarySummaries,
        diaryListRevision,
        currentId,
        currentDiary,
        withAttachments,
        currentDiaryAttachmentUrlMap,
        insertNewDiary,
        deleteSummary,
        updateAttachment,
        updateAttachmentFilename,
        deleteAttachment,
        invalidateDiaryList,
    }
});
