<template>
  <main id="unlock">
    <div class="unlock-container">
      <UnlockHeader />

      <div class="divider"></div>

      <div class="content-area">
        <LoadingState v-if="pipeline === 'wait-load-config'" />

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

        <ErrorState v-else />
      </div>
    </div>
  </main>
</template>

<script setup lang="ts">
import {onMounted, ref} from "vue";
import {useAppStore} from "../../stores/app.ts";
import {OssConfigType} from "../../types";
import {useRouter} from "vue-router";
import {showToast} from "../../utils";
import UnlockHeader from "./UnlockHeader.vue";
import LoadingState from "./LoadingState.vue";
import LoginSection from "./LoginSection.vue";
import ConfigSection from "./ConfigSection.vue";
import ErrorState from "./ErrorState.vue";

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

const appStore = useAppStore();
const router = useRouter();

function saveConfigAndLogin() {
  if (loading.value) return;
  loading.value = true;

  // 如果快速配置不为空 则解析快速配置
  if (quickConfig.value.trim() !== '') {
    if (masterPassword.value.trim() == '') {
      showToast("使用快速配置时，主密码不能为空。", 'error');
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

  appStore.saveConfigAndLogin(
      masterPassword.value,
      ossConfig.value,
  )
      .then(() => appStore.getEncryptedConfig())
      .then((ec) => {
        if (!ec) throw new Error('无法获取加密配置');
        encryptedConfig.value = ec;
        showToast("保存成功，请登录以验证主密码。", 'success');
        masterPassword.value = '';
        ossConfig.value = {
          akid: '',
          aks: '',
          bucket: '',
          endpoint: '',
        };
        pipeline.value = 'login';
      })
      .catch(err => showToast(`保存配置失败：${err.message || err}`, 'error'))
      .finally(() => loading.value = false);
}

function unlock() {
  if (loading.value) return;
  loading.value = true;
  appStore.unlock(masterPassword.value)
      .then(() => appStore.initOss(encryptedConfig.value))
      .then(() => {
        router.replace({name: 'DiaryList'});
        appStore.setTimeoutForCloseApp();
      })
      .catch(err => showToast(`解锁失败：${err.message || err}`, 'error'))
      .finally(() => loading.value = false);
}

function confirmReset() {
  // 确认是否重置
  if (confirm('确定要重置配置吗？这将删除所有本地配置。')) {
    appStore.resetConfig()
        .then(() => {
          pipeline.value = 'config';
          masterPassword.value = '';
        })
        .catch(err => showToast(`重置配置失败：${err.message || err}`, 'error'));
  }
}

onMounted(async () => {
  const ec = await appStore.getEncryptedConfig();
  if (ec) {
    pipeline.value = 'login';
    encryptedConfig.value = ec;
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
