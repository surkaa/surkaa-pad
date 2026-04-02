import {defineStore} from "pinia";
import {customRef, markRaw, onScopeDispose, Ref, ref} from "vue";
import {Store} from "@tauri-apps/plugin-store";
import {DEFAULT_THEME, ThemeType} from "../types.ts";
import {UnlistenFn} from "@tauri-apps/api/event";

const CONFIG_FILENAME = "settings.json";

type ConfigMap = {
    "app-theme": ThemeType;
    "biometric_enabled": boolean;
    "biometric_dek": string | null;
    "encrypted_oss_config": number[] | null;
    "default_image_size_is_small": boolean;
};
const DEFAULT_CONFIG = {
    "app-theme": DEFAULT_THEME,
    "biometric_enabled": false,
    "biometric_dek": null,
    "encrypted_oss_config": null,
    "default_image_size_is_small": false
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
            return DEFAULT_CONFIG[key];
        }
        return val;
    }

    // Vue 3 响应式 Hook：自动双向同步 Tauri Store，自动在组件卸载时清理监听
    function useTauriConfig<K extends ConfigKey>(key: K): Ref<ConfigMap[K]> {
        let val: ConfigMap[K] = DEFAULT_CONFIG[key];
        let unlisten: UnlistenFn | null = null;
        // 防抖标志，避免循环保存
        let isSyncing = false;

        const tauriRef = customRef((track, trigger) => {
            return {
                get() {
                    track();
                    return val;
                },
                set(newValue) {
                    val = newValue;
                    trigger();
                    // 如果不是来自底层的更新，则触发保存
                    if (!isSyncing) {
                        saveNormalConfig(key, newValue).catch(console.error);
                    }
                }
            };
        });

        // 异步初始化与监听
        initStore().then(async (s) => {
            const initial = await s.get<ConfigMap[K]>(key);
            if (initial !== null && initial !== undefined) {
                isSyncing = true;
                tauriRef.value = initial;
                isSyncing = false;
            }

            unlisten = await s.onKeyChange<ConfigMap[K]>(key, (newVal) => {
                isSyncing = true;
                tauriRef.value = newVal === null || newVal === undefined ? DEFAULT_CONFIG[key] : newVal;
                isSyncing = false;
            });
        });

        // 核心：当前组件上下文销毁时，自动清理 Tauri 的 Event Listener
        onScopeDispose(() => {
            if (unlisten) {
                unlisten();
                unlisten = null;
            }
        });

        return tauriRef;
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
        useTauriConfig,
        deleteConfig
    }

});
