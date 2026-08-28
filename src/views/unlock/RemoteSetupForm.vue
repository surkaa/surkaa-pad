<template>
  <q-form class="q-gutter-y-md" @submit.prevent="emit('submit')">
    <div class="text-h6 text-weight-bold form-title q-mb-sm">首次配置</div>
    <q-input
      :model-value="masterPassword"
      type="password"
      label="设置主密码"
      outlined
      color="primary"
      :disable="loading"
      :rules="[value => !!value || '主密码不能为空']"
      lazy-rules
      @update:model-value="updateText('update:masterPassword', $event)"
    >
      <template #prepend><q-icon name="vpn_key"/></template>
    </q-input>
    <q-input
      :model-value="confirmPassword"
      type="password"
      label="确认主密码"
      outlined
      color="primary"
      :disable="loading"
      :rules="[confirmPasswordRule]"
      lazy-rules
      @update:model-value="updateText('update:confirmPassword', $event)"
    >
      <template #prepend><q-icon name="verified_user"/></template>
    </q-input>
    <VaultKdfMemoryField
      :model-value="memoryCostKib"
      :loading="loading"
      remote-setup
      @update:model-value="emit('update:memoryCostKib', $event)"
    />
    <div class="row justify-center q-pb-sm">
      <q-btn
        flat
        rounded
        color="primary"
        :icon="showQuickInput ? 'list' : 'bolt'"
        :label="showQuickInput ? '使用常规配置' : '使用快速配置'"
        class="quick-mode-button"
        size="sm"
        @click="showQuickInput = !showQuickInput"
      />
    </div>
    <template v-if="!showQuickInput">
      <q-input :model-value="ossConfig.akid" label="AccessKey ID" outlined dense color="primary" :disable="loading"
               :rules="[value => !!value || '必填']" hide-bottom-space
               @update:model-value="updateOssConfig('akid', $event)"/>
      <q-input :model-value="ossConfig.aks" type="password" label="AccessKey Secret" outlined dense color="primary"
               :disable="loading" :rules="[value => !!value || '必填']" hide-bottom-space
               @update:model-value="updateOssConfig('aks', $event)"/>
      <q-input :model-value="ossConfig.bucket" label="Bucket 名称" outlined dense color="primary" :disable="loading"
               :rules="[value => !!value || '必填']" hide-bottom-space
               @update:model-value="updateOssConfig('bucket', $event)"/>
      <q-input :model-value="ossConfig.endpoint" label="Endpoint" outlined dense color="primary" :disable="loading"
               :rules="[value => !!value || '必填']" hide-bottom-space
               @update:model-value="updateOssConfig('endpoint', $event)"/>
    </template>
    <q-input
      v-else
      :model-value="quickConfig"
      type="textarea"
      label="快速配置内容"
      outlined
      color="primary"
      :disable="loading"
      rows="5"
      class="quick-config-input"
      placeholder="ALIYUN_KEY=xxx&#10;ALIYUN_SECRET=xxx&#10;ALIYUN_BUCKET_NAME=xxx&#10;ALIYUN_ENDPOINT=xxx"
      :rules="[value => !!value || '配置内容不能为空']"
      hide-bottom-space
      @update:model-value="updateText('update:quickConfig', $event)"
    />
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
</template>

<script setup lang="ts">
import {ref} from 'vue';
import type {OssConfigType} from '../../types';
import {masterPasswordConfirmationError} from '../../utils/masterPasswordSetup';
import VaultKdfMemoryField from './VaultKdfMemoryField.vue';

const props = defineProps<{
  masterPassword: string;
  confirmPassword: string;
  ossConfig: OssConfigType;
  quickConfig: string;
  memoryCostKib: number;
  loading: boolean;
}>();
const emit = defineEmits<{
  (event: 'update:masterPassword' | 'update:confirmPassword' | 'update:quickConfig', value: string): void;
  (event: 'update:ossConfig', value: OssConfigType): void;
  (event: 'update:memoryCostKib', value: number): void;
  (event: 'submit'): void;
}>();
const showQuickInput = ref(false);

function confirmPasswordRule(value: string) {
  return masterPasswordConfirmationError(props.masterPassword, value) || true;
}

function updateOssConfig(key: keyof OssConfigType, value: string | number | null) {
  emit('update:ossConfig', {...props.ossConfig, [key]: String(value ?? '')});
}

function updateText(
  event: 'update:masterPassword' | 'update:confirmPassword' | 'update:quickConfig',
  value: string | number | null,
) {
  emit(event, String(value ?? ''));
}
</script>

<style scoped lang="scss">
.form-title {
  color: var(--pad-text-color-200);
}

.quick-mode-button {
  color: var(--pad-primary-dark) !important;
  background: var(--pad-bg-color-100) !important;
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
</style>
