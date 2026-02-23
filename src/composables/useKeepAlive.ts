import {computed, ref} from 'vue';

const keepAliveIncludesRef = ref<string[]>(['DiaryList']); // 列表页默认常驻
export const keepAliveIncludes = computed(() => keepAliveIncludesRef.value);

export function addCache(name: string) {
    if (name && !keepAliveIncludesRef.value.includes(name)) {
        keepAliveIncludesRef.value.push(name);
    }
}

export function removeCache(name: string) {
    keepAliveIncludesRef.value = keepAliveIncludesRef.value.filter(n => n !== name);
}