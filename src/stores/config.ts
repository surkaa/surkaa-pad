import {defineStore} from "pinia";
import {markRaw, ref} from "vue";
import {Store} from "@tauri-apps/plugin-store";
import {DEFAULT_THEME, ThemeType} from "../types.ts";

const CONFIG_FILENAME = "settings.json";

type ConfigMap = {
    "app-theme": ThemeType;
    "biometric_enabled": boolean;
    "biometric_dek": string | null;
    "encrypted_oss_config": number[] | null;
};
const DEFAULT_CONFIG = {
    "app-theme": DEFAULT_THEME,
    "biometric_enabled": false,
    "biometric_dek": null,
    "encrypted_oss_config": null,
} satisfies ConfigMap;

type ConfigKey = keyof ConfigMap;

export const useConfigStore = defineStore('config', () => {
    let store = ref<Store>();

    async function initStore(): Promise<Store> {
        if (store.value) return store.value;
        const s = await Store.load(CONFIG_FILENAME);
        store.value = markRaw(s);
        return store.value;
    }

    async function saveNormalConfig<K extends ConfigKey>(key: K, value: ConfigMap[K]) {
        const s = await initStore();
        await s.set(key, value);
        await s.save();
    }

    async function getNormalConfig<K extends ConfigKey>(key: K): Promise<ConfigMap[K]> {
        const s = await initStore();
        const val = await s.get<ConfigMap[K]>(key);
        if (val === null || val === undefined) {
            await saveNormalConfig(key, DEFAULT_CONFIG[key]);
            return DEFAULT_CONFIG[key];
        }
        return val;
    }

    async function watchConfig<K extends ConfigKey>(
        key: K,
        callback: (value: ConfigMap[K] | undefined) => void,
        immediate?: boolean
    ) {
        const s = await initStore();
        if (immediate) {
            const val = await s.get<ConfigMap[K]>(key);
            callback(val === null || val === undefined ? DEFAULT_CONFIG[key] : val);
        }
        return await s.onKeyChange<ConfigMap[K]>(key, callback);
    }

    async function deleteConfig(...keys: ConfigKey[]): Promise<void> {
        const s = await initStore();
        for (const key of keys) {
            await s.delete(key);
        }
        await s.save();
    }

    return {
        saveNormalConfig,
        getNormalConfig,
        watchConfig,
        deleteConfig
    }

});
