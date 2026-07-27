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
      <span>生物识别已暂停，请输入一次主密码；之后 7 天内可继续使用</span>
    </div>
    <div class="q-mt-lg pt-md row justify-center">
      <q-btn flat color="grey-6" size="sm" label="重置配置" :disable="loading" @click="emit('reset')"/>
    </div>
  </q-form>
</template>

<script setup lang="ts">
defineProps<{
  masterPassword: string;
  loading: boolean;
  biometricEnabled: boolean;
  biometricUnlockAllowed: boolean;
}>();
const emit = defineEmits<{
  (event: 'update:masterPassword', value: string): void;
  (event: 'submit' | 'biometricUnlock' | 'reset'): void;
}>();
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
