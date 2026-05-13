import {defineStore} from "pinia";
import {customRef, markRaw, onScopeDispose, Ref, ref} from "vue";
import {Store} from "@tauri-apps/plugin-store";
import {DEFAULT_THEME, ThemeType} from "../types.ts";
import {UnlistenFn} from "@tauri-apps/api/event";
import {exists, readTextFile, writeTextFile, BaseDirectory} from "@tauri-apps/plugin-fs";

const CONFIG_FILENAME = "settings.json";
const BACKUP_FILENAME = "settings.json.bak";

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

export const useConfigStore = defineStore('config', () => {
    let store = ref<Store>();

    /**
     * 在 Store.load() 之前检查 settings.json 文件完整性。
     * 如果主文件损坏/为空但备份存在，从备份恢复。
     * 解决 Android 上进程被杀导致 fs::write 截断文件的问题。
     */
    async function ensureStoreIntegrity(): Promise<void> {
        try {
            const primaryExists = await exists(CONFIG_FILENAME, {baseDir: BaseDirectory.AppData});
            if (!primaryExists) {
                // 主文件不存在，检查备份
                const backupExists = await exists(BACKUP_FILENAME, {baseDir: BaseDirectory.AppData});
                if (backupExists) {
                    const backupContent = await readTextFile(BACKUP_FILENAME, {baseDir: BaseDirectory.AppData});
                    if (backupContent.trim().length > 0 && JSON.parse(backupContent)) {
                        await writeTextFile(CONFIG_FILENAME, backupContent, {baseDir: BaseDirectory.AppData});
                        console.warn('[config-store] 主配置文件不存在，已从备份恢复');
                    }
                }
                return;
            }

            // 主文件存在，检查内容是否有效
            const content = await readTextFile(CONFIG_FILENAME, {baseDir: BaseDirectory.AppData});
            if (content.trim().length === 0) {
                // 文件为空（被截断），从备份恢复
                const backupExists = await exists(BACKUP_FILENAME, {baseDir: BaseDirectory.AppData});
                if (backupExists) {
                    const backupContent = await readTextFile(BACKUP_FILENAME, {baseDir: BaseDirectory.AppData});
                    if (backupContent.trim().length > 0) {
                        await writeTextFile(CONFIG_FILENAME, backupContent, {baseDir: BaseDirectory.AppData});
                        console.warn('[config-store] 配置文件为空（可能被截断），已从备份恢复');
                    }
                }
                return;
            }

            // 文件有内容，验证是否为合法 JSON
            JSON.parse(content);
        } catch (e) {
            // JSON 解析失败 = 文件损坏，尝试从备份恢复
            console.warn('[config-store] 配置文件损坏，尝试从备份恢复:', e);
            try {
                const backupExists = await exists(BACKUP_FILENAME, {baseDir: BaseDirectory.AppData});
                if (backupExists) {
                    const backupContent = await readTextFile(BACKUP_FILENAME, {baseDir: BaseDirectory.AppData});
                    if (backupContent.trim().length > 0 && JSON.parse(backupContent)) {
                        await writeTextFile(CONFIG_FILENAME, backupContent, {baseDir: BaseDirectory.AppData});
                        console.warn('[config-store] 已从备份恢复损坏的配置文件');
                    }
                }
            } catch (backupErr) {
                console.error('[config-store] 备份恢复也失败:', backupErr);
            }
        }
    }

    async function initStore(): Promise<Store> {
        if (store.value) return store.value;

        // 加载前检查文件完整性，损坏时从备份恢复
        await ensureStoreIntegrity();

        const s = await Store.load(CONFIG_FILENAME);
        store.value = markRaw(s);

        // 加载成功后，如果有数据则创建备份
        try {
            if ((await s.length()) > 0) {
                const content = await readTextFile(CONFIG_FILENAME, {baseDir: BaseDirectory.AppData});
                if (content.trim().length > 0) {
                    await writeTextFile(BACKUP_FILENAME, content, {baseDir: BaseDirectory.AppData});
                }
            }
        } catch (e) {
            console.warn('[config-store] 创建配置备份失败:', e);
        }

        return store.value;
    }

    async function saveNormalConfig<K extends ConfigKey>(key: K, value: ConfigMap[K]) {
        const s = await initStore();
        await s.set(key, value);
        await s.save();
        // 每次成功保存后更新备份，确保备份与主文件同步
        try {
            const content = await readTextFile(CONFIG_FILENAME, {baseDir: BaseDirectory.AppData});
            if (content.trim().length > 0) {
                await writeTextFile(BACKUP_FILENAME, content, {baseDir: BaseDirectory.AppData});
            }
        } catch (e) {
            console.warn('[config-store] 更新配置备份失败:', e);
        }
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
        // 删除操作后也同步备份
        try {
            const content = await readTextFile(CONFIG_FILENAME, {baseDir: BaseDirectory.AppData});
            await writeTextFile(BACKUP_FILENAME, content, {baseDir: BaseDirectory.AppData});
        } catch (e) {
            console.warn('[config-store] 删除后更新配置备份失败:', e);
        }
    }

    return {
        saveNormalConfig,
        getNormalConfig,
        useTauriConfig,
        deleteConfig
    }

});
