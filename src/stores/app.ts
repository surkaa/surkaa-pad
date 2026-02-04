// --- 常量 ---
import {defineStore} from "pinia";
import {Store} from "@tauri-apps/plugin-store";
import {OssConfigType, ThemeType} from "../types";
import {markRaw, ref} from "vue";
import {window} from "@tauri-apps/api";
import {showToast} from "../utils";
import {biometricCipher} from "@tauri-apps/plugin-biometric";
import {commands} from "../bindings.ts";

const CONFIG_FILENAME = "settings.json";
const CONFIG_KEY = "encrypted_oss_config";
const SALE = 'NFI2cXl3cUpiSDk4bVVkdEY4cDMzRzlqcTdMMkY5WDg';
const THEME_KEY = 'app-theme';
const DEFAULT_THEME: ThemeType = 'system';
const BIOMETRIC_ENABLED_KEY = "biometric_enabled";
const BIOMETRIC_ENCRYPTED_DEK = "biometric_dek";
// 解锁后1小时自动关闭应用
const AUTO_CLOSE_APP_TIMEOUT = 60 * 60 * 1000;
// 时间到时剩余操作时间
const AUTO_CLOSE_APP_WARNING_TIME = 60 * 1000;

export const useAppStore = defineStore('app', () => {
    let store = ref<Store | null>(null);
    const keyword = ref<string>('');
    const savedScrollPosition = ref(0);
    const theme = ref<ThemeType>('system');
    const isBiometricEnabled = ref(false);
    let startTime: number = Date.now();

    function setTheme(t: ThemeType, save = true) {
        theme.value = t;
        save && saveNormalConfig(THEME_KEY, t).then();
    }

    async function initStore() {
        if (store.value) return;
        const s = await Store.load(CONFIG_FILENAME);
        store.value = markRaw(s);
        const theme = await getNormalConfig<ThemeType>(THEME_KEY);
        if (theme) {
            setTheme(theme, false);
        } else {
            setTheme(DEFAULT_THEME);
        }
        // 加载生物识别开关状态
        const enabled = await getNormalConfig<boolean>(BIOMETRIC_ENABLED_KEY);
        isBiometricEnabled.value = !!enabled;
    }

    async function getEncryptedConfig() {
        if (!store.value) {
            await initStore();
        }
        const val = await store.value!.get<number[]>(CONFIG_KEY);
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
        await commands.unlock(masterPassword, SALE);

        // 加密oss配置
        const configJson = JSON.stringify(ossConfig);
        const res = await commands.encryptData(configJson);
        if (res.status == 'error') {
            throw new Error(`加密配置失败: ${res.error}`);
        }
        const [encrypted_data, nonce] = res.data;

        const encryptedConfig = [...nonce, ...encrypted_data];

        // 保存加密后的配置
        await store.value.set(CONFIG_KEY, encryptedConfig);
        await store.value.save();
    }

    async function saveNormalConfig(key: string, value: any) {
        // 避免store为空
        if (!store.value) {
            await initStore();
        }
        await store.value!.set(key, value);
        await store.value!.save();
    }

    async function getNormalConfig<T>(key: string): Promise<T | null> {
        if (!store.value) {
            await initStore();
        }
        const val = await store.value!.get<T>(key);
        if (!val) return null;
        return val;
    }

    async function unlock(masterPassword: string) {
        await commands.unlock(masterPassword, SALE);
    }

    async function initOss(encryptedConfig: number[]) {
        const nonce = encryptedConfig.slice(0, 12);
        const ciphertext = encryptedConfig.slice(12);
        const res = await commands.decryptData(ciphertext, nonce);
        if (res.status == 'error') {
            throw new Error(`解密配置失败: ${res.error}`);
        }
        const ossJsonStr = res.data;
        const ossConfig = JSON.parse(ossJsonStr) as OssConfigType;
        const initRes = await commands.initOssClient(
            ossConfig.akid,
            ossConfig.aks,
            ossConfig.bucket,
            ossConfig.endpoint
        );
        if (initRes.status == 'error') {
            throw new Error(`初始化 OSS 客户端失败: ${initRes.error}`);
        }
    }

    function setTimeoutForCloseApp() {
        console.log('设置自动关闭应用定时器');
        setTimeout(() => {
            console.log('即将自动关闭应用');
            // 60s后退出
            showToast('一分钟后将自动关闭应用以保护数据安全', 'warning', AUTO_CLOSE_APP_WARNING_TIME)
            setTimeout(
                async () => await window.getCurrentWindow().close(),
                AUTO_CLOSE_APP_WARNING_TIME
            );
            setTimeout(() => {
                showToast('即将关闭应用以保护数据安全', 'error', AUTO_CLOSE_APP_WARNING_TIME / 2);
            }, AUTO_CLOSE_APP_WARNING_TIME / 2);
        }, AUTO_CLOSE_APP_TIMEOUT);
        startTime = Date.now();
    }

    function getEndTime() {
        // TODO 返回这个StartTime可能不是最新的
        return startTime + AUTO_CLOSE_APP_TIMEOUT;
    }

    async function resetConfig() {
        if (!store.value) {
            throw new Error('Store 未初始化');
        }
        await store.value.delete(CONFIG_KEY);
        await store.value.save();
    }

    async function searchWithKeyword(keyword: string): Promise<string[]> {
        const res = await commands.searchDiaries(keyword);
        if (res.status == 'error') {
            throw new Error(`搜索日记失败: ${res.error}`);
        }
        return res.data;
    }

    async function enableBiometric(masterPassword: string) {
        const res = await commands.unlock(masterPassword, SALE);
        if (res.status == 'error') {
            throw new Error(`解锁失败: ${res.error}`);
        }
        const dek = res.data;

        console.log('启用生物识别，获取到DEK：', dek);

        const response = await biometricCipher('请验证生物识别以启用快速解锁', {
            dataToEncrypt: dek
        });

        await saveNormalConfig(BIOMETRIC_ENCRYPTED_DEK, response.data);
        await saveNormalConfig(BIOMETRIC_ENABLED_KEY, true);
        isBiometricEnabled.value = true;
    }

    async function unlockWithBiometric() {
        const encryptedDek = await getNormalConfig<string>(BIOMETRIC_ENCRYPTED_DEK);
        if (!encryptedDek) throw new Error("未找到生物识别凭据");

        const {data} = await biometricCipher('请验证身份以解锁日记', {
            dataToDecrypt: encryptedDek
        });

        const res = await commands.biometricUnlock(data);
        if (res.status == 'error') {
            throw new Error(`生物识别解锁失败: ${res.error}`);
        }
    }

    async function disableBiometric() {
        if (!store.value) {
            await initStore();
        }
        await store.value!.delete(BIOMETRIC_ENCRYPTED_DEK);
        await store.value!.delete(BIOMETRIC_ENABLED_KEY);
        await store.value!.save();
        isBiometricEnabled.value = false;
    }

    return {
        // 数据
        keyword, savedScrollPosition, theme,
        // 方法
        getEncryptedConfig,
        unlock,
        initOss,
        saveConfigAndLogin,
        resetConfig,
        searchWithKeyword,
        setTimeoutForCloseApp,
        setTheme,
        initStore,
        getEndTime,
        isBiometricEnabled,
        enableBiometric,
        unlockWithBiometric,
        disableBiometric
    }
});
