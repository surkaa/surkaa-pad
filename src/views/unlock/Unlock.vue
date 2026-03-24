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

        <q-form
            v-else-if="pipeline === 'login'"
            @submit.prevent="unlock"
            class="q-gutter-y-lg q-pa-sm"
        >
          <div class="text-h6 text-weight-bold q-mb-sm" style="color: var(--pad-text-color)">欢迎回来</div>

          <q-input
              v-model="masterPassword"
              type="password"
              label="输入主密码"
              outlined
              autofocus
              color="primary"
              :disable="loading"
              :rules="[val => !!val || '请输入主密码']"
              lazy-rules
          />

          <q-btn
              type="submit"
              color="primary"
              class="full-width primary-gradient-btn"
              size="lg"
              :loading="loading"
              label="解锁"
              unelevated
          />

          <div class="q-mt-lg pt-md row justify-center">
            <q-btn flat color="grey-6" size="sm" label="重置配置" @click="confirmReset" :disable="loading"/>
          </div>
        </q-form>

        <q-form
            v-else-if="pipeline === 'config'"
            @submit.prevent="saveConfigAndLogin"
            class="q-gutter-y-md"
        >
          <div class="text-h6 text-weight-bold text-grey-9 q-mb-sm">首次配置</div>

          <q-input
              v-model="masterPassword"
              type="password"
              label="设置主密码"
              outlined
              color="primary"
              :disable="loading"
              :rules="[val => !!val || '主密码不能为空']"
              lazy-rules
          >
            <template v-slot:prepend>
              <q-icon name="vpn_key"/>
            </template>
          </q-input>

          <div class="row justify-center q-pb-sm">
            <q-btn
                flat
                rounded
                color="primary"
                :icon="showQuickInput ? 'list' : 'bolt'"
                :label="showQuickInput ? '使用常规配置' : '使用快速配置'"
                @click="showQuickInput = !showQuickInput"
                class="bg-grey-2"
                size="sm"
            />
          </div>

          <template v-if="!showQuickInput">
            <q-input v-model="ossConfig.akid" label="AccessKey ID" outlined dense color="primary" :disable="loading"
                     :rules="[val => !!val || '必填']" hide-bottom-space/>
            <q-input v-model="ossConfig.aks" type="password" label="AccessKey Secret" outlined dense color="primary"
                     :disable="loading" :rules="[val => !!val || '必填']" hide-bottom-space/>
            <q-input v-model="ossConfig.bucket" label="Bucket 名称" outlined dense color="primary" :disable="loading"
                     :rules="[val => !!val || '必填']" hide-bottom-space/>
            <q-input v-model="ossConfig.endpoint" label="Endpoint" outlined dense color="primary" :disable="loading"
                     :rules="[val => !!val || '必填']" hide-bottom-space/>
          </template>

          <template v-else>
            <q-input
                v-model="quickConfig"
                type="textarea"
                label="快速配置内容"
                outlined
                color="primary"
                :disable="loading"
                rows="5"
                class="quick-config-input"
                placeholder="ALIYUN_KEY=xxx&#10;ALIYUN_SECRET=xxx&#10;ALIYUN_BUCKET_NAME=xxx&#10;ALIYUN_ENDPOINT=xxx"
                :rules="[val => !!val || '配置内容不能为空']"
                hide-bottom-space
            />
          </template>

          <q-btn
              type="submit"
              color="primary"
              class="full-width primary-gradient-btn q-mt-lg"
              size="lg"
              :loading="loading"
              label="保存并登录"
              icon="save"
              unelevated
          />
        </q-form>

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

const $q = useQuasar();
const configStore = useConfigStore();
const {setTimeoutForCloseApp} = useTimeoutStore();
const router = useRouter();

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
const encryptedMemoryCost = ref(0);
const isAndroid = platform() === 'android';

async function saveConfigAndLogin() {
  if (loading.value) return;
  loading.value = true;

  // 如果快速配置不为空 则解析快速配置
  if (quickConfig.value.trim() !== '') {
    if (masterPassword.value.trim() == '') {
      $q.notify({type: 'warning', message: "使用快速配置时，主密码不能为空。"});
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

  try {
    await api.cmdUnlock(masterPassword.value);
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
    $q.notify({type: 'positive', message: "保存成功，请登录以验证主密码。"});
  } catch (e) {
    $q.notify({type: 'negative', message: `加密配置失败: ${formatError(e)}`});
    return;
  } finally {
    loading.value = false;
  }

  masterPassword.value = "";
  ossConfig.value = {akid: '', aks: '', bucket: '', endpoint: ''};
  pipeline.value = 'login';
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
    $q.notify({type: "negative", message: `初始化 OSS 客户端失败: ${formatError(e)}`});
    return false;
  }
}

async function unlock() {
  if (loading.value) return;
  loading.value = true;

  try {
    await api.cmdUnlock(masterPassword.value);

    if (!(await initOss())) {
      return;
    }

    console.log('Unlock Successful');
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
    await configStore.deleteConfig('encrypted_oss_config', 'biometric_enabled', 'biometric_dek');
    pipeline.value = 'config';
    masterPassword.value = '';
  }
}

async function tryBiometricUnlock() {
  if (loading.value) return;
  loading.value = true;
  try {
    const dataToDecrypt = await configStore.getNormalConfig('biometric_dek');
    if (!dataToDecrypt) {
      $q.notify({type: 'warning', message: "未找到生物识别凭据"});
      return;
    }

    const {data} = await biometricCipher('请验证身份以解锁日记', {dataToDecrypt});

    await api.cmdBiometricUnlock(data);

    if (!(await initOss())) {
      return;
    }

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
    if (isAndroid && await configStore.getNormalConfig('biometric_enabled')) {
      await nextTick(); // 等待加载完成再请求生物解锁
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

.primary-gradient-btn {
  background: var(--pad-primary-gradient) !important;
  border-radius: var(--pad-radius-lg);
  transition: all var(--pad-transition-base);

  &:hover:not(.disabled) {
    transform: translateY(-2px);
    box-shadow: var(--pad-shadow-md);
  }

  &:active:not(.disabled) {
    transform: translateY(0);
  }
}

.quick-config-input :deep(textarea) {
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
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
