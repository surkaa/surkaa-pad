// --- 常量 ---
import {defineStore} from "pinia";
import {Store} from "@tauri-apps/plugin-store";
import {EncryptData, OssConfigType, SearchIndexResult} from "../types";
import {invoke} from "@tauri-apps/api/core";
import {initOSS, listFiles} from "../utils/alioss.ts";
import {markRaw, ref} from "vue";

const CONFIG_FILENAME = "settings.json";
const CONFIG_KEY = "encrypted_oss_config";
const SALE = 'NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg';

export const useAppStore = defineStore('app', () => {
    let store = ref<Store | null>(null);
    const dek = ref<number[]>([]);

    async function getEncryptedConfig() {
        if (store.value) {
            const val = await store.value.get(CONFIG_KEY) as string;
            if (!val) return null;
            return val;
        }
        const s = await Store.load(CONFIG_FILENAME);
        store.value = markRaw(s);
        const val = await store.value.get(CONFIG_KEY) as string;
        if (!val) return null;
        return val;
    }

    async function saveConfigAndLogin(
        password: string,
        ossConfig: OssConfigType,
    ) {
        // 避免store为空
        if (!store.value) {
            throw new Error('Store 未初始化');
        }

        // 验证oss配置
        await initOSS(ossConfig);

        // 获取derivedKey
        const derivedKey = await invoke<number[]>('derive_key', {
            password,
            salt: SALE
        });

        // 加密oss配置
        const configJson = JSON.stringify(ossConfig);
        const encryptedConfig = await invoke<EncryptData>('encrypt_data', {
            plaintext: configJson,
            dek: derivedKey
        });

        // 保存加密后的配置
        await store.value.set(CONFIG_KEY, [
            ...encryptedConfig.nonce,
            ...encryptedConfig.ciphertext
        ]);
        await store.value.save();

        // 保存dek到状态
        dek.value = derivedKey;
    }

    async function unlock(masterPassword: string) {
        // 避免store为空
        if (!store.value) {
            throw new Error('Store 未初始化');
        }

        // 获取derivedKey
        const derivedKey = await invoke<number[]>('derive_key', {
            password: masterPassword,
            salt: SALE
        });

        // 获取加密的配置
        const encryptedConfig = await getEncryptedConfig();
        if (!encryptedConfig) {
            throw new Error('未找到加密的配置，请先登录');
        }

        // 分离nonce和ciphertext
        const nonceBytes = encryptedConfig.slice(0, 12);
        const ciphertext = encryptedConfig.slice(12);

        // 解密配置
        const decryptedConfigJson = await invoke<string>('decrypt_data', {
            dek: derivedKey,
            ciphertext,
            nonceBytes,
        });

        const ossConfig: OssConfigType = JSON.parse(decryptedConfigJson) as OssConfigType;

        // 初始化OSS客户端
        await initOSS(ossConfig);

        // 保存dek到状态
        dek.value = derivedKey;
    }

    async function resetConfig() {
        if (!store.value) {
            throw new Error('Store 未初始化');
        }
        await store.value.delete(CONFIG_KEY);
        await store.value.save();
        dek.value = [];
    }

    async function loadRemoteDiaryList() {
        const files = (await listFiles())
            .filter(fn => fn.endsWith('.dat'))
            .map(fn => Number(fn.replace('.dat', '')));
        return files.map(id => ({
            id,
            nonce: []
        }));
    }

    async function searchWithKeyword(searchTerm: string) {
        // 根据空格分词
        const keywords = searchTerm.split(' ').filter(k => k.trim() !== '');
        const matchedIds: Set<number> = new Set();
        for (const keyword of keywords) {
            const hash = await invoke<number[]>('generate_search_hash', {
                dek: dek.value,
                keyword
            });
            const results = await invoke<SearchIndexResult[]>('search_local_index', {
                searchHash: hash,
            });
            results.forEach(m => matchedIds.add(m.id));
        }
        return matchedIds;
    }

    return {
        getEncryptedConfig,
        saveConfigAndLogin,
        unlock,
        resetConfig,
        loadRemoteDiaryList,
        searchWithKeyword
    }
})