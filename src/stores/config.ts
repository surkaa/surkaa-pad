import {defineStore} from "pinia";
import {customRef, onScopeDispose, Ref} from "vue";
import {DEFAULT_THEME, ThemeType} from "../types.ts";
import {Store} from "@tauri-apps/plugin-store";

const STORAGE_PREFIX = 'config:';
const MIGRATION_KEY = 'config:migrated';
const CONFIG_FILENAME = "settings.json";

type ConfigMap = {
    "app-theme": ThemeType;
    "biometric_enabled": boolean;
    "biometric_dek": string | null;
    "encrypted_oss_config": number[] | null;
    "default_image_size_is_small": boolean;
    "pinned_diary_ids": string[]
};
const DEFAULT_CONFIG = {
    "app-theme": DEFAULT_THEME,
    "biometric_enabled": false,
    "biometric_dek": null,
    "encrypted_oss_config": null,
    "default_image_size_is_small": false,
    "pinned_diary_ids": []
} satisfies ConfigMap;

type ConfigKey = keyof ConfigMap;
const CONFIG_KEYS = Object.keys(DEFAULT_CONFIG) as ConfigKey[];

function storageKey(key: ConfigKey): string {
    return `${STORAGE_PREFIX}${key}`;
}

function readFromStorage<K extends ConfigKey>(key: K): ConfigMap[K] {
    const raw = localStorage.getItem(storageKey(key));
    if (raw === null) return DEFAULT_CONFIG[key];
    try {
        return JSON.parse(raw) as ConfigMap[K];
    } catch {
        return DEFAULT_CONFIG[key];
    }
}

/**
 * 从旧版 tauri-plugin-store 的 settings.json 迁移到 localStorage。
 * 只执行一次，迁移成功后在 localStorage 中设置标记。
 */
async function migrateFromStore(): Promise<void> {
    if (localStorage.getItem(MIGRATION_KEY)) return;

    try {
        const s = await Store.load(CONFIG_FILENAME);
        const len = await s.length();
        if (len === 0) {
            localStorage.setItem(MIGRATION_KEY, 'true');
            return;
        }

        for (const key of CONFIG_KEYS) {
            const val = await s.get<ConfigMap[typeof key]>(key);
            if (val !== null && val !== undefined) {
                localStorage.setItem(storageKey(key), JSON.stringify(val));
            }
        }
        localStorage.setItem(MIGRATION_KEY, 'true');
        console.info('[config-store] 已从 settings.json 迁移配置到 localStorage');
    } catch (e) {
        console.warn('[config-store] 迁移配置失败，使用默认值:', e);
        localStorage.setItem(MIGRATION_KEY, 'true');
    }
}

export const useConfigStore = defineStore('config', () => {
    let migratePromise: Promise<void> | null = null;

    function ensureMigrated(): Promise<void> {
        if (migratePromise) return migratePromise;
        migratePromise = migrateFromStore().finally(() => {
            // 迁移失败也不阻塞后续读写，下次会重试
        });
        return migratePromise;
    }

    async function saveNormalConfig<K extends ConfigKey>(key: K, value: ConfigMap[K]) {
        await ensureMigrated();
        localStorage.setItem(storageKey(key), JSON.stringify(value));
    }

    async function getNormalConfig<K extends ConfigKey>(key: K): Promise<ConfigMap[K]> {
        await ensureMigrated();
        return readFromStorage(key);
    }

    function useTauriConfig<K extends ConfigKey>(key: K): Ref<ConfigMap[K]> {
        let val: ConfigMap[K] = readFromStorage(key);
        let isSyncing = false;
        let triggerRef: (() => void) | null = null;

        const tauriRef = customRef<ConfigMap[K]>((track, trigger) => {
            triggerRef = trigger;
            return {
                get() {
                    track();
                    return val;
                },
                set(newValue) {
                    val = newValue;
                    trigger();
                    if (!isSyncing) {
                        localStorage.setItem(storageKey(key), JSON.stringify(newValue));
                    }
                }
            };
        });

        // 跨窗口同步
        const onStorage = (e: StorageEvent) => {
            if (e.key === storageKey(key)) {
                isSyncing = true;
                if (e.newValue === null) {
                    val = DEFAULT_CONFIG[key];
                } else {
                    try {
                        val = JSON.parse(e.newValue) as ConfigMap[K];
                    } catch {
                        val = DEFAULT_CONFIG[key];
                    }
                }
                triggerRef?.();
                isSyncing = false;
            }
        };
        window.addEventListener('storage', onStorage);

        onScopeDispose(() => {
            window.removeEventListener('storage', onStorage);
        });

        // 迁移完成后，用迁移来的值覆盖默认值（如果有变化）
        ensureMigrated().then(() => {
            const current = readFromStorage(key);
            if (JSON.stringify(current) !== JSON.stringify(val)) {
                isSyncing = true;
                val = current;
                triggerRef?.();
                isSyncing = false;
            }
        });

        return tauriRef;
    }

    async function deleteConfig(...keys: ConfigKey[]): Promise<void> {
        await ensureMigrated();
        for (const key of keys) {
            localStorage.removeItem(storageKey(key));
        }
    }

    return {
        saveNormalConfig,
        getNormalConfig,
        useTauriConfig,
        deleteConfig
    }
});
