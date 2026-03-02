import {defineStore} from 'pinia';
import {computed, ref} from 'vue';

export const useLayoutStore = defineStore('layout', () => {
    // 状态
    const customTitleRef = ref<string | null>(null);

    // 计算属性
    const customTitle = computed(() => {
        return customTitleRef.value;
    });

    // 动作
    function setTitle(title: string) {
        customTitleRef.value = title;
    }

    function resetTitle() {
        customTitleRef.value = null;
    }

    return {
        customTitle,
        setTitle,
        resetTitle
    };
});
