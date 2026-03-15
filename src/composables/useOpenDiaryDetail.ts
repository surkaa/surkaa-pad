import {useDataStore} from "../stores/data.ts";
import {useRouter} from "vue-router";
import {storeToRefs} from "pinia";

export function useOpenDiaryDetail() {
    const router = useRouter();
    const {currentDiaryAttachmentUrlMap, currentId} = storeToRefs(useDataStore());

    // 绑定到列表项点击
    async function openDiary(id?: string) {
        currentDiaryAttachmentUrlMap.value = {};
        if (!id) {
            // 新建日记
            currentId.value = "";
            await router.push({name: 'DiaryDetail'});
            return;
        }
        // 打开已有日记
        currentId.value = id;
        await router.push({name: 'DiaryDetail'});
    }

    return {openDiary}
}