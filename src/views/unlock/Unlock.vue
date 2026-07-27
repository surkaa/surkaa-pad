<template>
  <main id="unlock" class="flex flex-center">
    <q-card class="unlock-container text-center q-pa-none" flat>

      <q-card-section class="app-header q-py-lg">
        <div class="row items-center justify-center q-gutter-x-sm">
          <img alt="app-logo" class="app-logo" src="/app-icon.png"/>
          <div class="text-h5 text-weight-bold">{{ appName }}</div>
          <div class="text-subtitle1 text-weight-regular version-text">{{ version }}</div>
          <div class="text-subtitle2 version-text" style="opacity: 0.4">
            <q-icon name="security" size="14px" class="q-mr-xs" />
            {{ formatKiB(encryptedMemoryCost) }}
          </div>
        </div>
      </q-card-section>

      <q-card-section class="content-area q-pa-lg">

        <div v-if="pipeline === 'wait-load-config'" class="column items-center justify-center q-py-xl">
          <q-spinner color="primary" size="3em" :thickness="4"/>
          <div class="text-grey-7 q-mt-md text-subtitle1">正在加载配置...</div>
        </div>

        <FirstTimeUnlockForm
            v-else-if="pipeline === 'first-time'"
            v-model:master-password="masterPassword"
            v-model:confirm-password="confirmMasterPassword"
            :loading="loading"
            @submit="startLocalOnly"
            @configure-remote="pipeline = 'config'"
        />

        <PasswordLoginForm
            v-else-if="pipeline === 'login'"
            v-model:master-password="masterPassword"
            :loading="loading"
            :biometric-enabled="biometricEnabled"
            :biometric-unlock-allowed="biometricUnlockAllowed"
            @submit="unlock"
            @biometric-unlock="tryBiometricUnlock"
            @reset="confirmReset"
        />

        <RemoteSetupForm
            v-else-if="pipeline === 'config'"
            v-model:master-password="masterPassword"
            v-model:confirm-password="confirmMasterPassword"
            v-model:oss-config="ossConfig"
            v-model:quick-config="quickConfig"
            :loading="loading"
            @submit="saveConfigAndLogin"
        />

        <div v-else class="column items-center justify-center q-py-xl text-negative">
          <q-icon name="warning_amber" size="4em"/>
          <div class="text-h6 text-weight-bold q-mt-md">发生了未知的错误</div>
          <div class="text-grey-7">请刷新页面重试或检查应用状态</div>
        </div>

      </q-card-section>
    </q-card>
  </main>
</template>

<script setup lang="ts">
import {nextTick, onMounted, ref} from "vue";
import {OssConfigType} from "../../types.ts";
import {useRouter} from "vue-router";
import {getName, getVersion} from "@tauri-apps/api/app";
import {confirm} from '@tauri-apps/plugin-dialog';
import {useQuasar} from "quasar";
import {useTimeoutStore} from "../../stores/timeout.ts";
import {useConfigStore} from "../../stores/config.ts";
import {biometricCipher} from "@tauri-apps/plugin-biometric";
import api from "../../utils/api.ts";
import {formatError} from "../../utils/formatError.ts";
import {platform} from "@tauri-apps/plugin-os";
import {formatKiB} from "../../utils";
import {canUseBiometricUnlock} from "../../utils/biometricUnlockPolicy.ts";
import {masterPasswordConfirmationError} from "../../utils/masterPasswordSetup.ts";
import FirstTimeUnlockForm from './FirstTimeUnlockForm.vue';
import PasswordLoginForm from './PasswordLoginForm.vue';
import RemoteSetupForm from './RemoteSetupForm.vue';

const $q = useQuasar();
const configStore = useConfigStore();
const {setTimeoutForCloseApp} = useTimeoutStore();
const router = useRouter();

const pipeline = ref<'wait-load-config' | 'login' | 'config' | 'first-time'>('wait-load-config');
const encryptedConfig = ref<number[]>([]);
const ossConfig = ref<OssConfigType>({
  akid: '',
  aks: '',
  bucket: '',
  endpoint: '',
});
const quickConfig = ref('');
const masterPassword = ref<string>('');
const confirmMasterPassword = ref<string>('');
const loading = ref<boolean>(false);
const version = ref('0.0.0');
const appName = ref('App Name');
const encryptedMemoryCost = ref(0);
const isAndroid = platform() === 'android';
const biometricEnabled = ref(false);
const biometricUnlockAllowed = ref(false);

function validateInitialPasswordSetup(): boolean {
  const error = masterPasswordConfirmationError(
      masterPassword.value,
      confirmMasterPassword.value,
  );
  if (!error) return true;
  $q.notify({type: 'warning', message: error});
  return false;
}

async function recordPasswordUnlock() {
  try {
    await configStore.saveNormalConfig('last_password_unlock_at', Date.now());
  } catch (e) {
    console.warn(`记录主密码解锁时间失败: ${formatError(e)}`);
  }
}

async function refreshBiometricUnlockAllowed() {
  if (!biometricEnabled.value) {
    biometricUnlockAllowed.value = false;
    return;
  }

  const lastPasswordUnlockAt = await configStore.getNormalConfig('last_password_unlock_at');
  biometricUnlockAllowed.value = canUseBiometricUnlock(lastPasswordUnlockAt);
}

async function saveConfigAndLogin() {
  if (loading.value) return;
  if (!validateInitialPasswordSetup()) return;
  loading.value = true;

  // 如果快速配置不为空 则解析快速配置
  if (quickConfig.value.trim() !== '') {
    if (masterPassword.value.trim() == '') {
      $q.notify({type: 'warning', message: "使用快速配置时，主密码不能为空。"});
      loading.value = false;
      return;
    }
    // 去掉所有 \r，然后按字段名 = 定位提取值，不依赖换行符
    const raw = quickConfig.value.replace(/\r/g, '');
    const KEYS = ['ALIYUN_KEY', 'ALIYUN_SECRET', 'ALIYUN_BUCKET_NAME', 'ALIYUN_ENDPOINT'] as const;
    for (let i = 0; i < KEYS.length; i++) {
      const prefix = KEYS[i] + '=';
      const start = raw.indexOf(prefix);
      if (start === -1) continue;
      const valStart = start + prefix.length;
      // 下一个 key 的位置，没有则取到末尾
      let valEnd = raw.length;
      for (let j = i + 1; j < KEYS.length; j++) {
        const nextIdx = raw.indexOf(KEYS[j] + '=', valStart);
        if (nextIdx !== -1) { valEnd = nextIdx; break; }
      }
      const value = raw.slice(valStart, valEnd).replace(/\n/g, '').trim();
      switch (KEYS[i]) {
        case 'ALIYUN_KEY': ossConfig.value.akid = value; break;
        case 'ALIYUN_SECRET': ossConfig.value.aks = value; break;
        case 'ALIYUN_BUCKET_NAME': ossConfig.value.bucket = value; break;
        case 'ALIYUN_ENDPOINT': ossConfig.value.endpoint = value; break;
      }
    }

    const { akid, aks, bucket, endpoint } = ossConfig.value;
    if (!akid || !aks || !bucket || !endpoint) {
      $q.notify({ type: 'warning', message: '快速配置解析后仍有空字段，请检查内容格式是否正确' });
      loading.value = false;
      return;
    }
  }

  try {
    await api.cmdUnlock(masterPassword.value);
    await recordPasswordUnlock();
  } catch (e) {
    $q.notify({type: 'negative', message: `主密码验证失败: ${formatError(e)}`});
    loading.value = false;
    return;
  }

  // 加密oss配置
  const configJson = JSON.stringify(ossConfig.value);
  try {
    encryptedConfig.value = await api.cmdEncryptData(configJson);
    await configStore.saveNormalConfig('encrypted_oss_config', encryptedConfig.value);
  } catch (e) {
    $q.notify({type: 'negative', message: `加密配置失败: ${formatError(e)}`});
    loading.value = false;
    return;
  }

  // 直接验证 OSS 配置是否可用，跳过二次输入密码
  if (!(await initOss())) {
    // OSS 验证失败，清除已保存的配置让用户重试
    await configStore.deleteConfig('encrypted_oss_config');
    masterPassword.value = '';
    loading.value = false;
    return;
  }

  // OSS 配置成功，启用远程存储
  await configStore.saveNormalConfig('remote_enabled', true);
  await api.cmdSetRemoteEnabled(true);

  console.log('Setup & Unlock Successful');
  setTimeoutForCloseApp();
  await router.replace({name: 'DiaryList'});
}

async function initOss() {
  try {
    const res = await api.cmdDecryptData(encryptedConfig.value);
    const ossConfig = JSON.parse(res) as OssConfigType;
    await api.cmdInitOssClient(
        ossConfig.akid,
        ossConfig.aks,
        ossConfig.bucket,
        ossConfig.endpoint
    );
    return true;
  } catch (e) {
    console.error('[initOss] failed:', e);
    $q.notify({type: "negative", message: `初始化 OSS 客户端失败: ${formatError(e)}`});
    return false;
  }
}

async function unlock() {
  if (loading.value) return;
  loading.value = true;

  try {
    await api.cmdUnlock(masterPassword.value);
    await recordPasswordUnlock();

    const remoteEnabled = await configStore.getNormalConfig('remote_enabled');
    if (remoteEnabled) {
      if (!(await initOss())) {
        await api.cmdSetRemoteEnabled(false);
        return;
      }
    }
    await api.cmdSetRemoteEnabled(remoteEnabled);

    console.log('Unlock Successful');
    setTimeoutForCloseApp();
    await router.replace({name: 'DiaryList'});
  } catch (e) {
    $q.notify({type: "negative", message: `解锁失败: ${formatError(e)}`});
  } finally {
    loading.value = false;
  }
}

async function startLocalOnly() {
  if (loading.value) return;
  if (!validateInitialPasswordSetup()) return;
  loading.value = true;

  try {
    await api.cmdUnlock(masterPassword.value);
    await recordPasswordUnlock();
    await configStore.saveNormalConfig('remote_enabled', false);
    await api.cmdSetRemoteEnabled(false);

    console.log('Local-only Unlock Successful');
    setTimeoutForCloseApp();
    await router.replace({name: 'DiaryList'});
  } catch (e) {
    $q.notify({type: "negative", message: `解锁失败: ${formatError(e)}`});
  } finally {
    loading.value = false;
  }
}

async function confirmReset() {
  if (await confirm('确定要重置OssClient配置吗？这将删除所有本地配置。')) {
    await configStore.deleteConfig(
        'encrypted_oss_config',
        'remote_enabled',
        'biometric_enabled',
        'biometric_dek',
        'last_password_unlock_at',
    );
    pipeline.value = 'first-time';
    masterPassword.value = '';
    confirmMasterPassword.value = '';
  }
}

async function tryBiometricUnlock() {
  if (loading.value) return;
  loading.value = true;
  try {
    await refreshBiometricUnlockAllowed();
    if (!biometricUnlockAllowed.value) {
      $q.notify({type: 'warning', message: '本周需要使用主密码解锁一次'});
      return;
    }

    const dataToDecrypt = await configStore.getNormalConfig('biometric_dek');
    if (!dataToDecrypt) {
      $q.notify({type: 'warning', message: "未找到生物识别凭据"});
      return;
    }

    const {data} = await biometricCipher('请验证身份以解锁日记', {dataToDecrypt});

    await api.cmdBiometricUnlock(data);

    const remoteEnabled = await configStore.getNormalConfig('remote_enabled');
    if (remoteEnabled) {
      if (!(await initOss())) {
        await api.cmdSetRemoteEnabled(false);
        return;
      }
    }
    await api.cmdSetRemoteEnabled(remoteEnabled);

    console.log('Biometric Unlock Successful');
    setTimeoutForCloseApp();
    await router.replace({name: 'DiaryList'});
  } catch (e) {
    console.warn(`生物识别未成功或被取消: ${formatError(e)}`);
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  version.value = await getVersion();
  appName.value = await getName();
  encryptedMemoryCost.value = await api.cmdEncryptInfo();
  const ec = await configStore.getNormalConfig('encrypted_oss_config');
  if (ec) {
    pipeline.value = 'login';
    encryptedConfig.value = ec;
    biometricEnabled.value = isAndroid
        && await configStore.getNormalConfig('biometric_enabled');
    await refreshBiometricUnlockAllowed();
    if (biometricUnlockAllowed.value) {
      await nextTick(); // 等待加载完成再请求生物解锁
      await tryBiometricUnlock();
    }
  } else {
    pipeline.value = 'first-time';
  }
});
</script>

<style scoped lang="scss">
#unlock {
  width: 100%;
  height: 100%;
  background-color: var(--pad-bg-color-100);
  font-family: var(--pad-font-family), serif;
  padding: 5%;
  box-sizing: border-box;
  user-select: none;
}

.unlock-container {
  width: 100%;
  max-width: 512px;
  background-color: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
  border: 1px solid var(--pad-border-color-100);
  box-shadow: var(--pad-shadow-lg);
  overflow: hidden;
  animation: container-enter 0.5s ease-out;
}

.app-header {
  background: var(--pad-primary-gradient);
  color: var(--pad-text-color-light);

  .app-logo {
    width: 48px;
    height: 48px;
    filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.2));
  }

  .version-text {
    opacity: 0.8;
  }
}

@keyframes container-enter {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

// 响应式设计
@media (max-width: 512px) {
  #unlock {
    padding: 5% 0;

    .unlock-container {
      border-radius: 0;
      border: none;
      box-shadow: none;
    }
  }
}

@media (min-width: 513px) and (max-width: 768px) {
  #unlock {
    padding: 8%;

    .unlock-container {
      max-width: 420px;
    }
  }
}
</style>
