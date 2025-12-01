<template>
  <main id="unlock">
    <h1>SurKaa-Pad</h1>
    <!-- 分界线 -->
    <hr>
    <section v-if="pipeline === 'wait-load-config'" id="wait-load-config">
      正在加载配置...
    </section>

    <section v-else-if="pipeline === 'login'" id="login">
      <h5>欢迎回来</h5>

      <!-- 表单 -->
      <form @submit.prevent="unlock">
        <input id="master-password" type="password" required placeholder="输入输密码解锁" v-model="masterPassword">
        <button type="submit" :disabled="loading" :class="{'loading': loading}">
          {{ loading ? '正在验证...' : '解锁' }}
        </button>
      </form>
      <p class="link-btn" @click="confirmReset">重置配置</p>
    </section>

    <section v-else-if="pipeline === 'config'" id="config">
      <h5>首次配置</h5>

      <!-- 表单 -->
      <form @submit.prevent="saveConfigAndLogin">
        <input id="master-password" type="password" required placeholder="Master Password" v-model="masterPassword">
        <input id="access-key-id" type="text" required placeholder="AccessKey ID" v-model="ossConfig.accessKeyId">
        <input id="access-key-secret" type="password" required placeholder="AccessKey Secret"
               v-model="ossConfig.accessKeySecret">
        <input id="bucket-name" type="text" required placeholder="Bucket" v-model="ossConfig.bucket">
        <input id="endpoint" type="text" required placeholder="Endpoint" v-model="ossConfig.endpoint">
        <input id="region" type="text" required placeholder="Region" v-model="ossConfig.region">
        <button type="submit" :disabled="loading" :class="{'loading': loading}">
          {{ loading ? '正在验证并保存...' : '保存并登录' }}
        </button>
      </form>
    </section>

    <section v-else id="unknown-error">
      发生了未知的错误。
    </section>
  </main>
</template>

<script setup lang="ts">
import {onMounted, ref} from "vue";
import {useAppStore} from "../stores/app.ts";
import {OssConfigType} from "../types";
import {useRouter} from "vue-router";

const pipeline = ref<'wait-load-config' | 'login' | 'config'>('wait-load-config');
const encryptedConfig = ref<number[]>([]);
const ossConfig = ref<OssConfigType>({
  accessKeyId: '',
  accessKeySecret: '',
  bucketName: '',
  endpoint: '',
});
const masterPassword = ref<string>('');
const loading = ref<boolean>(false);

const appStore = useAppStore();
const router = useRouter();

function saveConfigAndLogin() {
  if (loading.value) return;
  loading.value = true;

  appStore.saveConfigAndLogin(
      masterPassword.value,
      ossConfig.value,
  )
      .then(() => pipeline.value = 'login')
      .catch(err => alert(`保存配置失败：${err.message || err}`))
      .finally(() => loading.value = false);
}

function unlock() {
  if (loading.value) return;
  loading.value = true;
  appStore.unlock(masterPassword.value)
      .then(() => appStore.initOss(encryptedConfig.value))
      .then(() => router.replace({name: 'diary-list'}))
      .catch(err => alert(`解锁失败：${err.message || err}`))
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
        .catch(err => alert(`重置配置失败：${err.message || err}`));
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
  --padding: clamp(16px, 4vw, 48px);
  width: calc(100% - 2 * var(--padding));
  height: calc(100% - 2 * var(--padding));
  display: flex;
  justify-content: center;
  align-items: center;
  flex-direction: column;

  h1 {
    width: 100%;
    text-align: left;
    font-size: 32px;
    color: var(--pad-text-color-100);
  }

  section {
    flex: 1; // 占据剩下的全部高度
    width: 100%;
    font-size: 24px;
    color: var(--pad-text-color-200);
  }

  #login {
    display: flex;
    flex-direction: column;
    justify-content: start;
    align-items: center;

    h5 {
      width: 100%;
      text-align: left;
      font-size: 20px;
      margin-bottom: 16px;
    }

    form {
      width: 100%;
      display: flex;
      flex-direction: column;
      gap: 12px;

      input {
        padding: 1rem;
        font-size: 16px;
        border: 1px solid var(--pad-border-color-200);
        border-radius: 4px;
        background-color: var(--pad-bg-color-100);
        color: var(--pad-text-color-100);
      }

      button {
        width: 100%;
        padding: 10px 0;
        font-size: 16px;
        border: none;
        border-radius: 4px;
        background-color: var(--pad-bg-color-400);
        color: var(--pad-text-color-100);
        cursor: pointer;

        &:hover {
          background-color: var(--pad-bg-color-500);
        }

        // loading时hover无效
        &.loading:hover {
          background-color: var(--pad-bg-color-400);
          cursor: not-allowed;
        }

        &.loading {
          opacity: 0.7;
        }
      }
    }

    .link-btn {
      width: 100%;
      color: var(--pad-text-color-500);
      cursor: pointer;
      font-size: 0.9rem;
      text-decoration: underline;
      margin-top: auto;
      text-align: left;
    }
  }

  #config {
    // 暂时和登录页一样 未来可能会有区别
    display: flex;
    flex-direction: column;
    justify-content: start;
    align-items: center;

    h5 {
      width: 100%;
      text-align: left;
      font-size: 20px;
      margin-bottom: 16px;
    }

    form {
      width: 100%;
      display: flex;
      flex-direction: column;
      gap: 12px;

      input {
        width: 100%;
        padding: 8px 12px;
        font-size: 16px;
        border: 1px solid var(--pad-border-color-200);
        border-radius: 4px;
        background-color: var(--pad-bg-color-100);
        color: var(--pad-text-color-100);
      }

      button {
        width: 100%;
        padding: 10px 0;
        font-size: 16px;
        border: none;
        border-radius: 4px;
        background-color: var(--pad-bg-color-400);
        color: var(--pad-text-color-100);
        cursor: pointer;

        &:hover {
          background-color: var(--pad-bg-color-500);
        }

        // loading时hover无效
        &.loading:hover {
          background-color: var(--pad-bg-color-400);
          cursor: not-allowed;
        }

        &.loading {
          opacity: 0.7;
        }
      }
    }
  }
}
</style>
