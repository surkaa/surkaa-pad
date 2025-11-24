import { defineStore } from 'pinia';
import {markRaw, ref} from 'vue';
import { invoke } from "@tauri-apps/api/core";
import { Store } from "@tauri-apps/plugin-store";
import { initOSS, listFiles } from "../utils/alioss"; // 假设 alioss.ts 在 src/utils 下
import { EncryptData, DiaryEntry, OSSConfig } from "../types"; // 假设 types.ts 在 src 下

// --- 常量 ---
const CONFIG_FILENAME = "settings.json";
const CONFIG_KEY = "encrypted_oss_config";
const SALT_BASE64 = "NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg"; // 固定的硬编码胡椒值

export const useAppStore = defineStore('app', () => {
    // --- 存储实例 ---
    const localStore = ref<Store | null>(null);
    const isLoadingDerivedKey = ref(false);

    // --- 状态变量 ---
    const hasSavedConfig = ref(false); // 是否存在本地配置
    const isLoggedIn = ref(false);     // 是否已登录
    const statusMessage = ref('初始化中...');
    const dek = ref<number[]>([]);       // 派生密钥 (DEK)
    const ossConfig = ref<OSSConfig>({    // OSS 配置，用于初始化 alioss
        accessKeyId: '',
        accessKeySecret: '',
        region: 'cn-guangzhou',
        endpoint: 'oss-cn-guangzhou.aliyuncs.com',
        bucket: 'surkaa'
    });

    // --- 业务数据 ---
    const diaryList = ref<DiaryEntry[]>([]); // 日记列表
    const viewMode = ref<'list' | 'editor'>('list'); // 当前视图模式 ('list' | 'editor')
    const currentEntryId = ref<number | null>(null);   // 当前编辑的 ID (空代表新建)

    // ==========================================
    // 动作 (Actions)
    // ==========================================

    async function initializeStore() {
        if (!localStore.value) {
            try {
                // 1. 在 onMounted 中加载 Store
                const loadedStore = await Store.load(CONFIG_FILENAME);

                // 2. 使用 markRaw 包装 Store 实例，解决 TypeError
                localStore.value = markRaw(loadedStore);

                // 3. 检查配置
                await checkSavedConfig();
            } catch (e) {
                console.error("Store 加载失败:", e);
                statusMessage.value = "本地存储初始化失败。";
                throw e;
            }
        }
    }

    async function checkSavedConfig() {
        if (!localStore.value) return;
        try {
            const val = await localStore.value.get(CONFIG_KEY);
            hasSavedConfig.value = !!val;
            statusMessage.value = hasSavedConfig.value
                ? "发现本地配置，请输入密码解锁。"
                : "无本地配置，请进行首次设置。";
        } catch (e) {
            console.error("读取配置失败:", e);
        }
    }

    async function resetConfig() {
        if (!localStore.value) return;
        await localStore.value.delete(CONFIG_KEY);
        await localStore.value.save();
        hasSavedConfig.value = false;
        isLoggedIn.value = false;
        dek.value = [];
        statusMessage.value = "配置已重置。";
    }

    // 辅助函数：派生密钥
    async function deriveKey(masterPassword: string): Promise<number[]> {
        if (isLoadingDerivedKey.value) {
            statusMessage.value = "正在处理中...";
            throw new Error("正在派生密钥，请勿重复操作");
        }
        isLoadingDerivedKey.value = true;

        try {
            return await invoke<number[]>('derive_key', {
                password: masterPassword,
                salt: SALT_BASE64
            });
        } finally {
            isLoadingDerivedKey.value = false;
        }
    }

    // 1. 首次设置和登录
    async function handleFirstSetup(password: string, config: OSSConfig) {
        if (!localStore.value) throw new Error("Store 未初始化");
        statusMessage.value = "正在验证配置...";

        try {
            const derivedKey = await deriveKey(password);

            // 1. 初始化 OSS
            await initOSS(config);

            // 2. 加密配置
            const configJson = JSON.stringify(config);
            const ed = await invoke<EncryptData>('encrypt_data', {
                dek: derivedKey,
                plaintext: configJson
            });

            // 3. 存储加密配置
            await localStore.value.set(CONFIG_KEY, [...ed.nonce, ...ed.ciphertext]);
            await localStore.value.save();

            // 4. 更新状态
            dek.value = derivedKey;
            ossConfig.value = config;
            isLoggedIn.value = true;
            hasSavedConfig.value = true;
            statusMessage.value = "登录成功。";

            await loadDiaryList();

        } catch (e) {
            statusMessage.value = `设置失败: ${e}`;
            throw e;
        }
    }

    // 2. 解锁
    async function handleUnlock(password: string) {
        if (!localStore.value) throw new Error("Store 未初始化");
        statusMessage.value = "正在解锁...";

        try {
            const derivedKey = await deriveKey(password);
            const encryptedConfig = await localStore.value.get<number[]>(CONFIG_KEY);
            if (!encryptedConfig) throw new Error("配置文件丢失");

            // 1. 解密配置
            const nonceBytes = encryptedConfig.slice(0, 12);
            const ciphertext = encryptedConfig.slice(12);
            const configJson = await invoke<string>('decrypt_data', {
                dek: derivedKey,
                ciphertext,
                nonceBytes
            });

            const config: OSSConfig = JSON.parse(configJson);

            // 2. 初始化 OSS
            await initOSS(config);

            // 3. 更新状态
            dek.value = derivedKey;
            ossConfig.value = config;
            isLoggedIn.value = true;
            statusMessage.value = "解锁成功。";

            await loadDiaryList();

        } catch (e) {
            statusMessage.value = `解锁失败: ${e}`;
            throw e;
        }
    }

    // 3. 加载日记列表
    async function loadDiaryList() {
        if (!isLoggedIn.value) return;
        try {
            // 这里我们依赖的是 OSS 文件列表，而不是本地 DB 列表，
            // 稍后在同步功能中我们会纠正这一点，现在先保持原样。
            const files = (await listFiles())
                .filter((fn: string) => fn.endsWith('.dat'))
                .map((fn: string) => Number(fn.replace('.dat', '')));

            diaryList.value = files
                .map((id: number) => ({ id, nonce: [] }));
        } catch (e) {
            console.error("加载列表失败", e);
            statusMessage.value = `列表加载失败: ${e}`;
        }
    }

    // 4. 新建日记
    function openNewEntry() {
        currentEntryId.value = null;
        viewMode.value = 'editor';
        statusMessage.value = '新建日记模式';
    }


    return {
        // 状态
        hasSavedConfig,
        isLoggedIn,
        statusMessage,
        dek,
        ossConfig,
        viewMode,
        diaryList,
        currentEntryId,
        isLoadingDerivedKey,
        // 动作
        initializeStore,
        resetConfig,
        handleFirstSetup,
        handleUnlock,
        loadDiaryList,
        openNewEntry
    };
});