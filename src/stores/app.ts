// --- 常量 ---
import {defineStore} from "pinia";
import {Store} from "@tauri-apps/plugin-store";

const CONFIG_FILENAME = "settings.json";
const CONFIG_KEY = "encrypted_oss_config";

export const useAppStore = defineStore('app', () => {
    let store: Store | null = null;

    async function getEncryptedConfig() {
        store = await Store.get(CONFIG_FILENAME);
        if (!store) return null;
        const val = await store.get(CONFIG_KEY) as string;
        if (!val) return null;
        return val;
    }

    return {
        getEncryptedConfig,
    }
})