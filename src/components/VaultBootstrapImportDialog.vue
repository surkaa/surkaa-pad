<template>
  <q-dialog
    :model-value="modelValue"
    persistent
    no-refocus
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="import-dialog">
      <q-card-section>
        <div class="text-h6 title-text">导入密钥派生配置</div>
        <div class="text-caption desc-text">
          粘贴完整 JSON 并输入对应主密码。后端会验证配置及已有本地数据，全部通过前不会保存。
        </div>
      </q-card-section>
      <q-card-section class="q-pt-none q-gutter-y-md">
        <q-input
          v-model="importJson"
          type="textarea"
          outlined
          autogrow
          label="配置 JSON"
          :disable="importing"
        />
        <q-input
          v-model="importPassword"
          type="password"
          outlined
          label="对应的主密码"
          :disable="importing"
          @keyup.enter="importConfig"
        />
      </q-card-section>
      <q-card-actions align="right" class="q-px-md q-pb-md">
        <q-btn flat label="取消" class="secondary-action" :disable="importing" @click="close"/>
        <q-btn
          unelevated
          label="验证并导入"
          color="primary"
          :loading="importing"
          :disable="!importJson.trim() || !importPassword"
          @click="importConfig"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import type {VaultBootstrap} from '../bindings';
import api from '../utils/api';
import {formatError} from '../utils/formatError';

const props = withDefaults(defineProps<{
  modelValue: boolean;
  initialPassword?: string;
}>(), {
  initialPassword: '',
});
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'imported', bootstrap: VaultBootstrap, password: string): void;
}>();
const $q = useQuasar();
const importJson = ref('');
const importPassword = ref('');
const importing = ref(false);

watch(() => props.modelValue, shown => {
  if (!shown) return;
  importJson.value = '';
  importPassword.value = props.initialPassword;
});

function close() {
  if (!importing.value) emit('update:modelValue', false);
}

async function importConfig() {
  if (!importJson.value.trim() || !importPassword.value || importing.value) return;
  importing.value = true;
  try {
    const password = importPassword.value;
    const bootstrap = await api.cmdImportVaultBootstrap(importJson.value.trim(), password);
    emit('imported', bootstrap, password);
    emit('update:modelValue', false);
    $q.notify({type: 'positive', message: '密钥派生配置验证并导入成功'});
  } catch (error) {
    $q.notify({type: 'negative', message: `导入配置失败：${formatError(error)}`});
  } finally {
    importing.value = false;
  }
}
</script>

<style scoped lang="scss">
.import-dialog {
  width: min(620px, calc(100vw - 24px));
  max-width: 620px;
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
}

.title-text {
  color: var(--pad-text-color-200);
}

.desc-text {
  color: var(--pad-text-color-400);
}

</style>
