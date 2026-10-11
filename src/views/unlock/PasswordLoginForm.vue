<template>
  <q-form class="q-gutter-y-lg q-pa-sm" @submit.prevent="emit('submit')">
    <div class="text-h6 text-weight-bold q-mb-sm form-title">欢迎回来</div>
    <q-input
      :model-value="masterPassword"
      type="password"
      label="输入主密码"
      outlined
      autofocus
      color="primary"
      :disable="loading"
      :rules="[value => !!value || '请输入主密码']"
      lazy-rules
      @update:model-value="emit('update:masterPassword', String($event ?? ''))"
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
    <q-btn
      v-if="biometricUnlockAllowed"
      type="button"
      outline
      color="primary"
      class="full-width"
      size="md"
      icon="fingerprint"
      label="使用生物识别解锁"
      :loading="loading"
      @click="emit('biometricUnlock')"
    />
    <div
      v-if="biometricEnabled && !biometricUnlockAllowed"
      class="row items-center justify-center q-gutter-x-xs text-caption password-required-hint"
    >
      <q-icon name="schedule" size="16px"/>
      <span>生物识别已暂停，请输入一次主密码；之后 {{ biometricPasswordIntervalDays }} 天内可继续使用</span>
    </div>
    <div class="q-mt-lg pt-md row items-center justify-center q-gutter-x-sm">
      <q-btn flat color="primary" size="sm" label="导入密钥配置" :disable="loading" @click="emit('importBootstrap')"/>
      <q-btn flat color="grey-6" size="sm" label="重置配置" :disable="loading" @click="emit('reset')"/>
      <q-btn v-if="showChangedKeyHint" flat round color="primary" size="sm" icon="error_outline" aria-label="其他设备改密后的解锁说明" :disable="loading" @click="showKeyChangeHelp" />
    </div>
  </q-form>
</template>

<script setup lang="ts">
import {useQuasar} from 'quasar';

defineProps<{
  masterPassword: string;
  loading: boolean;
  biometricEnabled: boolean;
  biometricUnlockAllowed: boolean;
  biometricPasswordIntervalDays: number;
  showChangedKeyHint?: boolean;
}>();
const emit = defineEmits<{
  (event: 'update:masterPassword', value: string): void;
  (event: 'submit' | 'biometricUnlock' | 'reset' | 'importBootstrap'): void;
}>();
const $q = useQuasar();
function showKeyChangeHelp() {
  $q.dialog({
    title: '其他设备已改密？',
    message: '若其他设备已改密，请先输入本机原密码，再按提示输入新密码以更新本机配置。',
    ok: {label: '知道了', color: 'primary'},
  });
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

.password-required-hint {
  color: var(--pad-text-color-400);
  line-height: 1.5;
}
</style>
