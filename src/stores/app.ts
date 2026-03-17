import {defineStore} from "pinia";
import {Store} from "@tauri-apps/plugin-store";
import {OssConfigType, ThemeType} from "../types";
import {markRaw, ref} from "vue";
import {biometricCipher} from "@tauri-apps/plugin-biometric";
import {commands} from "../bindings.ts";
import {useQuasar} from "quasar";

// --- 常量 ---
const CONFIG_FILENAME = "settings.json";
const CONFIG_KEY = "encrypted_oss_config";
const THEME_KEY = 'app-theme';
const DEFAULT_THEME: ThemeType = 'system';
const BIOMETRIC_ENABLED_KEY = "biometric_enabled";
const BIOMETRIC_ENCRYPTED_DEK = "biometric_dek";

export const useAppStore = defineStore('app', () => {
    let store = ref<Store | null>(null);

    const $q = useQuasar();
    const theme = ref<ThemeType>('system');
    const isBiometricEnabled = ref(false);

    function setTheme(t: ThemeType, save = true) {
        theme.value = t;
        save && saveNormalConfig(THEME_KEY, t).then();
        // 设置Quasar的主题
        if (t === 'light') {
            $q.dark.set(false);
        } else if (t === 'dark') {
            $q.dark.set(true);
        } else {
            // 跟随系统
            $q.dark.set('auto');
        }
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
        await commands.cmdUnlock(masterPassword);

        // 加密oss配置
        const configJson = JSON.stringify(ossConfig);
        const res = await commands.cmdEncryptData(configJson);
        if (res.status == 'error') {
            throw new Error(`加密配置失败: ${res.error}`);
        }

        const encryptedConfig = res.data;

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
        await commands.cmdUnlock(masterPassword);
    }

    async function initOss(encryptedConfig: number[]) {
        const res = await commands.cmdDecryptData(encryptedConfig);
        if (res.status == 'error') {
            throw new Error(`解密配置失败: ${res.error}`);
        }
        const ossJsonStr = res.data;
        const ossConfig = JSON.parse(ossJsonStr) as OssConfigType;
        const initRes = await commands.cmdInitOssClient(
            ossConfig.akid,
            ossConfig.aks,
            ossConfig.bucket,
            ossConfig.endpoint
        );
        if (initRes.status == 'error') {
            throw new Error(`初始化 OSS 客户端失败: ${initRes.error}`);
        }
    }

    async function resetConfig() {
        if (!store.value) {
            throw new Error('Store 未初始化');
        }
        await store.value.delete(CONFIG_KEY);
        await store.value.save();
    }

    async function enableBiometric(masterPassword: string) {
        const res = await commands.cmdUnlock(masterPassword);
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

        const res = await commands.cmdBiometricUnlock(data);
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
        theme,
        // 方法
        getEncryptedConfig,
        unlock,
        initOss,
        saveConfigAndLogin,
        resetConfig,
        setTheme,
        initStore,
        isBiometricEnabled,
        enableBiometric,
        unlockWithBiometric,
        disableBiometric
    }
});
