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
      :dark="$q.dark.isActive"
      :options-dark="$q.dark.isActive"
      popup-content-class="kdf-memory-select-popup"
      :disable="loading"
      @update:model-value="emit('update:modelValue', Number($event))"
    >
      <template #prepend><q-icon name="memory"/></template>
    </q-select>
    <div class="field-hint">{{ VAULT_KDF_COST_HINT }}</div>
    <div v-if="remoteSetup" class="field-hint">仅在云端为空时用于创建新 Vault；已有 Vault 会采用云端参数。</div>
    <div v-if="developmentEdition && modelValue === KIB_PER_MIB" class="field-hint">{{ DEV_VAULT_KDF_COST_HINT }}</div>
  </div>
</template>

<script setup lang="ts">
import {useQuasar} from 'quasar';
import {computed} from 'vue';
import {DEV_VAULT_KDF_COST_HINT, KIB_PER_MIB, newVaultMemoryOptions, VAULT_KDF_COST_HINT} from '../../utils/vaultKdfSetup';
import {useDevelopmentEdition} from '../../composables/useDevelopmentEdition';

defineProps<{
  modelValue: number;
  loading: boolean;
  remoteSetup?: boolean;
}>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: number): void;
}>();

const $q = useQuasar();
const developmentEdition = useDevelopmentEdition();
const options = computed(() => newVaultMemoryOptions(developmentEdition.value));
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

:global(.kdf-memory-select-popup) {
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
}

:global(.kdf-memory-select-popup .q-item) {
  color: var(--pad-text-color-200);
}

</style>
