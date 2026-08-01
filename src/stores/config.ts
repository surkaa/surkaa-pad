import {defineStore} from "pinia";
import {customRef, onScopeDispose, Ref} from "vue";
import {DEFAULT_THEME, ThemeType} from "../types.ts";
import {
    DEFAULT_WINDOWS_EDITOR_SHORTCUTS,
    type EditorShortcutConfig,
} from "../utils/editorShortcuts.ts";
import {
    DEFAULT_UPLOAD_CONCURRENCY,
    normalizeUploadConcurrency,
} from '../utils/uploadConcurrency';
import {
    DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS,
    normalizeDiaryListShortcutConfig,
    type DiaryListShortcutConfig,
} from '../utils/diaryListShortcuts';

const STORAGE_PREFIX = 'config:';

type ConfigMap = {
    "app-theme": ThemeType;
    "biometric_enabled": boolean;
    "biometric_dek": string | null;
    "last_password_unlock_at": number | null;
    "vault_verifier": number[] | null;
    "encrypted_oss_config": number[] | null;
    "encrypted_ai_config": number[] | null;
    "default_image_size_is_small": boolean;
    "encrypt_image_attachments": boolean;
    "encrypt_audio_attachments": boolean;
    "encrypt_video_attachments": boolean;
    "encrypt_file_attachments": boolean;
    "attachment_upload_concurrency": number;
    "pinned_diary_ids": string[]
    "windows_editor_shortcuts": EditorShortcutConfig;
    "windows_diary_list_shortcuts": DiaryListShortcutConfig;
};
const DEFAULT_CONFIG = {
    "app-theme": DEFAULT_THEME,
    "biometric_enabled": false,
    "biometric_dek": null,
    "last_password_unlock_at": null,
    "vault_verifier": null,
    "encrypted_oss_config": null,
    "encrypted_ai_config": null,
    "default_image_size_is_small": false,
    "encrypt_image_attachments": true,
    "encrypt_audio_attachments": true,
    "encrypt_video_attachments": true,
    "encrypt_file_attachments": true,
    "attachment_upload_concurrency": DEFAULT_UPLOAD_CONCURRENCY,
    "pinned_diary_ids": [],
    "windows_editor_shortcuts": {...DEFAULT_WINDOWS_EDITOR_SHORTCUTS},
    "windows_diary_list_shortcuts": {...DEFAULT_WINDOWS_DIARY_LIST_SHORTCUTS},
} satisfies ConfigMap;

type ConfigKey = keyof ConfigMap;

function storageKey(key: ConfigKey): string {
    return `${STORAGE_PREFIX}${key}`;
}

function normalizeConfigValue<K extends ConfigKey>(key: K, value: unknown): ConfigMap[K] {
    if (key === 'attachment_upload_concurrency') {
        return normalizeUploadConcurrency(value) as ConfigMap[K];
    }
    if (key === 'windows_diary_list_shortcuts') {
        return normalizeDiaryListShortcutConfig(value) as ConfigMap[K];
    }
    return value as ConfigMap[K];
}

function readFromStorage<K extends ConfigKey>(key: K): ConfigMap[K] {
    const raw = localStorage.getItem(storageKey(key));
    if (raw === null) {
        return DEFAULT_CONFIG[key];
    }
    try {
        return normalizeConfigValue(key, JSON.parse(raw));
    } catch {
        return DEFAULT_CONFIG[key];
    }
}

/** 同窗口自定义事件名，解决 storage 事件不通知自身窗口的问题 */
const SAME_WINDOW_EVENT = 'config:local-change';

interface ConfigChangeDetail {
    key: string;
    newValue: string | null; // null = 已删除
}

function writeToStorage(key: ConfigKey, value: unknown) {
    const sk = storageKey(key);
    const json = JSON.stringify(normalizeConfigValue(key, value));
    localStorage.setItem(sk, json);
    window.dispatchEvent(new CustomEvent<ConfigChangeDetail>(SAME_WINDOW_EVENT, {
        detail: { key: sk, newValue: json }
    }));
}

function removeFromStorage(key: ConfigKey) {
    const sk = storageKey(key);
    localStorage.removeItem(sk);
    window.dispatchEvent(new CustomEvent<ConfigChangeDetail>(SAME_WINDOW_EVENT, {
        detail: { key: sk, newValue: null }
    }));
}

export const useConfigStore = defineStore('config', () => {
    async function saveNormalConfig<K extends ConfigKey>(key: K, value: ConfigMap[K]) {
        writeToStorage(key, value);
    }

    async function getNormalConfig<K extends ConfigKey>(key: K): Promise<ConfigMap[K]> {
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
                        writeToStorage(key, newValue);
                    }
                }
            };
        });

        function handleChange(newValue: string | null) {
            isSyncing = true;
            if (newValue === null) {
                val = DEFAULT_CONFIG[key];
            } else {
                try {
                    val = normalizeConfigValue(key, JSON.parse(newValue));
                } catch {
                    val = DEFAULT_CONFIG[key];
                }
            }
            triggerRef?.();
            isSyncing = false;
        }

        // 跨窗口同步（storage 事件只在其他窗口触发）
        const onStorage = (e: StorageEvent) => {
            if (e.key === storageKey(key)) {
                handleChange(e.newValue);
            }
        };
        window.addEventListener('storage', onStorage);

        // 同窗口同步（storage 事件不通知自身窗口，用自定义事件弥补）
        const onLocalChange = (e: Event) => {
            const detail = (e as CustomEvent<ConfigChangeDetail>).detail;
            if (detail.key === storageKey(key)) {
                handleChange(detail.newValue);
            }
        };
        window.addEventListener(SAME_WINDOW_EVENT, onLocalChange);

        onScopeDispose(() => {
            window.removeEventListener('storage', onStorage);
            window.removeEventListener(SAME_WINDOW_EVENT, onLocalChange);
        });

        return tauriRef;
    }

    async function deleteConfig(...keys: ConfigKey[]): Promise<void> {
        for (const key of keys) {
            removeFromStorage(key);
        }
    }

    async function getLegacyRemoteEnabled(): Promise<boolean> {
        const raw = localStorage.getItem(`${STORAGE_PREFIX}remote_enabled`);
        if (raw !== null) {
            try {
                return JSON.parse(raw) === true;
            } catch {
                return false;
            }
        }
        return localStorage.getItem(storageKey('encrypted_oss_config')) !== null;
    }

    async function deleteLegacyRemoteEnabled(): Promise<void> {
        localStorage.removeItem(`${STORAGE_PREFIX}remote_enabled`);
    }

    return {
        saveNormalConfig,
        getNormalConfig,
        useTauriConfig,
        deleteConfig,
        getLegacyRemoteEnabled,
        deleteLegacyRemoteEnabled,
    }
});
