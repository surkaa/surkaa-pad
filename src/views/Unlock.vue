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
      配置界面内容...
    </section>

    <section v-else id="unknown-error">
      发生了未知的错误。
    </section>
  </main>
</template>

<script setup lang="ts">
import {onMounted, ref} from "vue";
import {useAppStore} from "../stores/app.ts";

const pipeline = ref<'wait-load-config' | 'login' | 'config'>('wait-load-config');
const encryptedConfig = ref<string | null>(null);

const appStore = useAppStore();

onMounted(async () => {
  const ec = await appStore.getEncryptedConfig();
  if (ec) {
    pipeline.value = 'login';
    encryptedConfig.value = ec;
  } else {
    pipeline.value = 'config';
  }
})
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
    display: flex;
    justify-content: center;
    align-items: start;
    font-size: 24px;
    color: var(--pad-text-color-200);
  }
}
</style>