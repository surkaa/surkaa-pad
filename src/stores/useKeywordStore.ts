import {ref} from "vue";

export function useKeywordStore() {
    const keyword = ref<string>('');

    return {
        keyword
    }
}