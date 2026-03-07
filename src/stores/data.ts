import {defineStore} from "pinia";
import {computed, ref} from "vue";
import {AttachmentMeta, DiarySummary} from "../bindings.ts";

export const useDataStore = defineStore('data', () => {
    const diaryIds = ref<string[]>([]);
    const diarySummaries = ref<Record<string, DiarySummary | null>>({});

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
    }

    function updateAttachment(diaryId: string, newMeta: AttachmentMeta) {
        const summary = diarySummaries.value[diaryId];
        if (summary) {
            const attachmentIndex = summary.attachments.findIndex(att => att.filename === newMeta.filename);
            if (attachmentIndex !== -1) {
                summary.attachments[attachmentIndex] = newMeta;
            } else {
                summary.attachments.push(newMeta);
            }
        }
    }

    function deleteAttachment(diaryId: string, filenames: string[]) {
        const summary = diarySummaries.value[diaryId];
        if (summary) {
            summary.attachments = summary.attachments.filter(att => !filenames.includes(att.filename));
        }
    }

    return {
        diaryIds,
        diarySummaries,
        currentId,
        currentDiary,
        withAttachments,
        currentDiaryAttachmentUrlMap,
        insertNewDiary,
        deleteSummary,
        updateAttachment,
        deleteAttachment,
    }
});
