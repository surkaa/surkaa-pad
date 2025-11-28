<template>
  <main id="unlock">
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
  width: 100%;
  height: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
}
</style>