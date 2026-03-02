import {computed, ref} from "vue";

export function useKeywordStore() {
    const keywordInner = ref<string>('');

    const keyword = computed(() => keywordInner.value);

    function setKeyword(k: string) {
        keywordInner.value = k;
    }

    return {
        keyword,
        setKeyword
    }
}