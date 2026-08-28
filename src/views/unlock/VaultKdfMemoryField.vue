<template>
  <div class="kdf-memory-field">
    <q-select
      :model-value="modelValue"
      :options="options"
      label="密钥派生内存"
      outlined
      emit-value
      map-options
      options-dense
      color="primary"
      :disable="loading"
      popup-content-class="vault-kdf-options"
      @update:model-value="emit('update:modelValue', Number($event))"
    >
      <template #prepend><q-icon name="memory"/></template>
    </q-select>
    <div class="field-hint">
      {{ remoteSetup
        ? '仅在云端为空时用于创建新 Vault；已有 Vault 会采用云端参数。'
        : '数值越高越难暴力破解，也会增加解锁时的内存占用。创建后不能直接修改。' }}
    </div>
  </div>
</template>

<script setup lang="ts">
import {newVaultMemoryOptions} from '../../utils/vaultKdfSetup';

defineProps<{
  modelValue: number;
  loading: boolean;
  remoteSetup?: boolean;
}>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: number): void;
}>();

const options = newVaultMemoryOptions();
</script>

<style scoped lang="scss">
.kdf-memory-field {
  display: grid;
  gap: 5px;
}

.field-hint {
  padding: 0 12px;
  color: var(--pad-text-color-400);
  font-size: 0.75rem;
  line-height: 1.45;
  text-align: left;
}

:deep(.q-field__native),
:deep(.q-field__input),
:deep(.q-field__label),
:deep(.q-field__marginal) {
  color: var(--pad-text-color-200);
}

:deep(.q-field__control::before) {
  border-color: var(--pad-border-color-100);
}
</style>

<style lang="scss">
.vault-kdf-options {
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
  border: 1px solid var(--pad-border-color-100);
}
</style>
