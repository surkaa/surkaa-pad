<template>
  <main id="unlock">
    <div class="unlock-container">
      <UnlockHeader :version :appName/>

      <div class="divider"></div>

      <div class="content-area">
        <LoadingState v-if="pipeline === 'wait-load-config'"/>

        <LoginSection
            v-else-if="pipeline === 'login'"
            v-model:masterPassword="masterPassword"
            :loading="loading"
            @unlock="unlock"
            @reset="confirmReset"
        />

        <ConfigSection
            v-else-if="pipeline === 'config'"
            v-model:masterPassword="masterPassword"
            :ossConfig="ossConfig"
            v-model:quickConfig="quickConfig"
            v-model:showQuickInput="showQuickInput"
            :loading="loading"
            @save="saveConfigAndLogin"
        />

        <ErrorState v-else/>
      </div>
    </div>
  </main>
</template>

<script setup lang="ts">
import {onMounted, ref} from "vue";
import {OssConfigType} from "../../types.ts";
import {useRouter} from "vue-router";
import UnlockHeader from "./UnlockHeader.vue";
import LoadingState from "./LoadingState.vue";
import LoginSection from "./LoginSection.vue";
import ConfigSection from "./ConfigSection.vue";
import ErrorState from "./ErrorState.vue";
import {getName, getVersion} from "@tauri-apps/api/app";
import {confirm} from '@tauri-apps/plugin-dialog';
import {useQuasar} from "quasar";
import {useTimeoutStore} from "../../stores/timeout.ts";
import {useConfigStore} from "../../stores/config.ts";
import {commands} from "../../bindings.ts";
import {biometricCipher} from "../../../../Forks/tauri-plugins-workspace/plugins/biometric";

const pipeline = ref<'wait-load-config' | 'login' | 'config'>('wait-load-config');
const encryptedConfig = ref<number[]>([]);
const ossConfig = ref<OssConfigType>({
  akid: '',
  aks: '',
  bucket: '',
  endpoint: '',
});
const showQuickInput = ref<boolean>(false);
const quickConfig = ref('');
const masterPassword = ref<string>('');
const loading = ref<boolean>(false);
const version = ref('0.0.0');
const appName = ref('App Name');

const $q = useQuasar();
const configStore = useConfigStore();
const {setTimeoutForCloseApp} = useTimeoutStore();
const router = useRouter();

async function saveConfigAndLogin() {
  if (loading.value) return;
  loading.value = true;

  // 如果快速配置不为空 则解析快速配置
  if (quickConfig.value.trim() !== '') {
    if (masterPassword.value.trim() == '') {
      $q.notify("使用快速配置时，主密码不能为空。");
      loading.value = false;
      return;
    }
    console.log('使用快速配置：', quickConfig.value);
    const lines = quickConfig.value.split('\n').filter(line => line.includes('='));
    lines.forEach(line => {
      const [key, value] = line.split('=').map(s => s.trim());
      switch (key) {
        case 'ALIYUN_KEY':
          ossConfig.value.akid = value;
          break;
        case 'ALIYUN_SECRET':
          ossConfig.value.aks = value;
          break;
        case 'ALIYUN_BUCKET_NAME':
          ossConfig.value.bucket = value;
          break;
        case 'ALIYUN_ENDPOINT':
          ossConfig.value.endpoint = value;
          break;
        default:
          console.warn('未知的配置项：', key);
          loading.value = false;
          return;
      }
    });
  }

  await commands.cmdUnlock(masterPassword.value);

  // 加密oss配置
  const configJson = JSON.stringify(ossConfig.value);
  const res = await commands.cmdEncryptData(configJson);
  if (res.status == 'error') {
    throw new Error(`加密配置失败: ${res.error}`);
  }

  encryptedConfig.value = res.data;
  await configStore.saveNormalConfig('encrypted_oss_config', encryptedConfig.value);
  $q.notify("保存成功，请登录以验证主密码。");
  masterPassword.value = "";
  ossConfig.value = {
    akid: '',
    aks: '',
    bucket: '',
    endpoint: '',
  };
  pipeline.value = 'login';
  loading.value = false;
}

async function unlock() {
  if (loading.value) return;
  loading.value = true;
  const unlockRes = await commands.cmdUnlock(masterPassword.value);
  if (unlockRes.status == 'error') {
    $q.notify({type: "negative", message: `解锁失败: ${unlockRes.error}`});
    loading.value = false;
    return;
  }
  const res = await commands.cmdDecryptData(encryptedConfig.value);
  if (res.status == 'error') {
    $q.notify({type: "negative", message: `解密配置失败: ${res.error}`});
    loading.value = false;
    return;
  }
  const ossConfig = JSON.parse(res.data) as OssConfigType;
  const initRes = await commands.cmdInitOssClient(
      ossConfig.akid,
      ossConfig.aks,
      ossConfig.bucket,
      ossConfig.endpoint
  );
  if (initRes.status == 'error') {
    $q.notify({type: "negative", message: `初始化 OSS 客户端失败: ${initRes.error}`});
    loading.value = false;
    return;
  }
  console.log('Unlock Successful');
  loading.value = false;
  setTimeoutForCloseApp();
  await router.replace({name: 'DiaryList'});
}

async function confirmReset() {
  // 确认是否重置
  if (await confirm('确定要OssClient配置吗？这将删除所有本地配置。')) {
    await configStore.deleteConfig('encrypted_oss_config', 'biometric_enabled', 'biometric_dek');
    pipeline.value = 'config';
    masterPassword.value = '';
  }
}

async function tryBiometricUnlock() {
  loading.value = true;
  try {
    const dataToDecrypt = await configStore.getNormalConfig('biometric_dek');
    if (!dataToDecrypt) {
      $q.notify({type: 'negative', message: "未找到生物识别凭据"});
      return;
    }

    const {data} = await biometricCipher('请验证身份以解锁日记', {dataToDecrypt});

    const res = await commands.cmdBiometricUnlock(data);
    if (res.status == 'error') {
      $q.notify({type: 'negative', message: `生物识别解锁失败: ${res.error}`});
      return;
    }
    const decryptRes = await commands.cmdDecryptData(encryptedConfig.value);
    if (decryptRes.status == 'error') {
      $q.notify({type: 'negative', message: `解密配置失败: ${decryptRes.error}`});
      return;
    }
    const ossConfig = JSON.parse(decryptRes.data) as OssConfigType;
    const initRes = await commands.cmdInitOssClient(
        ossConfig.akid,
        ossConfig.aks,
        ossConfig.bucket,
        ossConfig.endpoint
    );
    if (initRes.status == 'error') {
      $q.notify({type: 'negative', message: `初始化 OSS 客户端失败: ${initRes.error}`});
      return;
    }

    $q.notify("生物识别解锁成功");
    setTimeoutForCloseApp();
    await router.replace({name: 'DiaryList'});
  } catch (e: any) {
    // 用户取消或失败，不做处理，留在登录界面让用户输密码
    console.log("生物识别未通过:", e);
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  version.value = await getVersion();
  appName.value = await getName();
  const ec = await configStore.getNormalConfig('encrypted_oss_config');
  if (ec) {
    pipeline.value = 'login';
    encryptedConfig.value = ec;
    if (await configStore.getNormalConfig('biometric_enabled')) {
      await tryBiometricUnlock();
    }
  } else {
    pipeline.value = 'config';
  }
});
</script>

<style scoped lang="scss">
#unlock {
  width: 100%;
  height: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
  background-color: var(--pad-bg-color-100);
  font-family: var(--pad-font-family), serif;
  padding: 5%;
  box-sizing: border-box;
  user-select: none;

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

  .divider {
    height: 1px;
    background: var(--pad-border-color-100);
    margin: 0;
  }

  .content-area {
    padding: 32px;
  }
}

// 动画定义
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
    }

    .content-area {
      padding: 24px 20px;
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
