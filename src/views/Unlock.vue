<template>
  <main id="unlock">
    <h1>SurKaa-Pad</h1>
    <!-- 分界线 -->
    <hr>
    <section v-if="pipeline === 'wait-load-config'" id="wait-load-config">
      正在加载配置...
    </section>

    <section v-else-if="pipeline === 'login'" id="login">
      请进行登录操作...
    </section>

    <section v-else-if="pipeline === 'config'" id="config">
      <h5>首次配置</h5>

      <!-- 表单 -->
      <form @submit.prevent="saveConfigAndLogin">
        <input id="master-password" type="password" required placeholder="Master Password" v-model="masterPassword">
        <input id="access-key-id" type="text" required placeholder="AccessKey ID" v-model="ossConfig.accessKeyId">
        <input id="access-key-secret" type="password" required placeholder="AccessKey Secret" v-model="ossConfig.accessKeySecret">
        <input id="bucket-name" type="text" required placeholder="Bucket" v-model="ossConfig.bucket">
        <input id="endpoint" type="text" required placeholder="Endpoint" v-model="ossConfig.endpoint">
        <input id="region" type="text" required placeholder="Region" v-model="ossConfig.region">
        <button type="submit">保存并登录</button>
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

const pipeline = ref<'wait-load-config' | 'login' | 'config'>('wait-load-config');
const encryptedConfig = ref<string | null>(null);
const ossConfig = ref<OssConfigType>({
  accessKeyId: '',
  accessKeySecret: '',
  bucket: '',
  endpoint: '',
  region: ''
});
const masterPassword = ref<string>('');

const appStore = useAppStore();

function saveConfigAndLogin() {

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

  #config {
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
      }
    }
  }
}
</style>