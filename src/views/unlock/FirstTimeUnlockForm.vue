<template>
  <q-form class="q-gutter-y-lg q-pa-sm" @submit.prevent="emit('submit')">
    <div class="text-h6 text-weight-bold q-mb-sm form-title">开始使用</div>
    <q-input
      :model-value="masterPassword"
      type="password"
      label="设置主密码"
      outlined
      autofocus
      color="primary"
      :disable="loading"
      :rules="[value => !!value || '主密码不能为空']"
      lazy-rules
      @update:model-value="updateText('update:masterPassword', $event)"
    />
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
    />
    <VaultKdfMemoryField
      :model-value="memoryCostKib"
      :loading="loading"
      @update:model-value="emit('update:memoryCostKib', $event)"
    />
    <q-btn
      type="submit"
      color="primary"
      class="full-width primary-gradient-btn"
      size="lg"
      :loading="loading"
      label="开始使用（本地存储）"
      unelevated
    />
    <div class="q-mt-md row justify-center">
      <q-btn flat color="primary" size="sm" label="导入密钥配置" :disable="loading" @click="emit('importBootstrap')"/>
      <q-btn flat color="primary" size="sm" label="配置云存储" :disable="loading" @click="emit('configureRemote')"/>
    </div>
  </q-form>
</template>

<script setup lang="ts">
import {masterPasswordConfirmationError} from '../../utils/masterPasswordSetup';
import VaultKdfMemoryField from './VaultKdfMemoryField.vue';

const props = defineProps<{
  masterPassword: string;
  confirmPassword: string;
  memoryCostKib: number;
  loading: boolean;
}>();
const emit = defineEmits<{
  (event: 'update:masterPassword' | 'update:confirmPassword', value: string): void;
  (event: 'update:memoryCostKib', value: number): void;
  (event: 'submit' | 'configureRemote' | 'importBootstrap'): void;
}>();

function confirmPasswordRule(value: string) {
  return masterPasswordConfirmationError(props.masterPassword, value) || true;
}

function updateText(event: 'update:masterPassword' | 'update:confirmPassword', value: string | number | null) {
  emit(event, String(value ?? ''));
}
</script>

<style scoped lang="scss">
.form-title {
  color: var(--pad-text-color);
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
</style>
