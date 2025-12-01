// --- 常量 ---
import {defineStore} from "pinia";
import {Store} from "@tauri-apps/plugin-store";
import {DiaryManifest, OssConfigType} from "../types";
import {invoke} from "@tauri-apps/api/core";
import {markRaw, ref} from "vue";

const CONFIG_FILENAME = "settings.json";
const CONFIG_KEY = "encrypted_oss_config";
const SALE = 'NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg';

export const useAppStore = defineStore('app', () => {
    let store = ref<Store | null>(null);

    async function getEncryptedConfig() {
        if (store.value) {
            const val = await store.value.get<number[]>(CONFIG_KEY);
            if (!val) return null;
            return val;
        }
        const s = await Store.load(CONFIG_FILENAME);
        store.value = markRaw(s);
        const val = await store.value.get<number[]>(CONFIG_KEY);
        if (!val) return null;
        return val;
    }

    async function saveConfigAndLogin(
        masterPassword: string,
        ossConfig: OssConfigType,
    ) {
        // 避免store为空
        if (!store.value) {
            throw new Error('Store 未初始化');
        }

        // 解锁
        await invoke<number[]>('unlock', {
            masterPassword,
            salt: SALE
        });

        // 加密oss配置
        const configJson = JSON.stringify(ossConfig);
        const [encrypted_data, nonce] = await invoke<[number[], number[]]>('encrypt_data', {
            data: configJson
        });

        const encryptedConfig = [...nonce, ...encrypted_data];

        // 保存加密后的配置
        await store.value.set(CONFIG_KEY, encryptedConfig);
        await store.value.save();
    }

    async function unlock(masterPassword: string) {
        await invoke<number[]>('unlock', {
            masterPassword,
            salt: SALE
        });
    }

    async function initOss(encryptedConfig: number[]) {
        const nonce = encryptedConfig.slice(0, 12);
        const ciphertext = encryptedConfig.slice(12);
        const ossJsonStr = await invoke<string>('decrypt_data', {
            ciphertext,
            nonce
        });
        const ossConfig = JSON.parse(ossJsonStr) as OssConfigType;
        await invoke('init_oss_client', {...ossConfig});
    }

    async function resetConfig() {
        if (!store.value) {
            throw new Error('Store 未初始化');
        }
        await store.value.delete(CONFIG_KEY);
        await store.value.save();
    }

    async function loadLocalDiaries(): Promise<DiaryManifest[]> {
        return await invoke<DiaryManifest[]>('list_local_diaries');
    }

    async function searchWithKeyword(keyword: string): Promise<string[]> {
        return await invoke<string[]>('search_diaries', {keyword});
    }

    async function syncFromOss() {
        return await invoke<void>('sync_from_oss');
    }

    return {
        getEncryptedConfig,
        unlock,
        initOss,
        saveConfigAndLogin,
        resetConfig,
        loadLocalDiaries,
        searchWithKeyword,
        syncFromOss
    }
})