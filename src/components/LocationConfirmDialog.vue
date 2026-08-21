<script setup lang="ts">
import {computed, ref, watch} from 'vue';
import type {DiaryLocation} from '../bindings';
import {formatLocationCoordinates} from '../utils/diaryLocation';

const props = defineProps<{
  modelValue: boolean;
  location: DiaryLocation | null;
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'retry'): void;
  (event: 'confirm', value: DiaryLocation): void;
}>();

const placeName = ref('');

watch(
  () => [props.modelValue, props.location] as const,
  ([visible, location]) => {
    if (visible && location) placeName.value = location.placeName || '';
  },
  {immediate: true},
);

const coordinates = computed(() => props.location
  ? formatLocationCoordinates(props.location)
  : '');
const accuracy = computed(() => {
  const value = props.location?.horizontalAccuracyMeters;
  return typeof value === 'number' ? `约 ±${Math.round(value)} 米` : '未知';
});
const capturedTime = computed(() => props.location
  ? new Date(props.location.capturedAt).toLocaleString()
  : '');

function close() {
  emit('update:modelValue', false);
}

function confirm() {
  if (!props.location) return;
  emit('confirm', {
    ...props.location,
    placeName: placeName.value.trim() || null,
  });
}
</script>

<template>
  <q-dialog
    :model-value="modelValue"
    no-refocus
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="location-dialog-card">
      <q-card-section class="location-dialog-heading">
        <div class="location-dialog-icon">
          <q-icon name="location_on" size="24px"/>
        </div>
        <div>
          <div class="text-h6">记录当前位置</div>
          <div class="location-dialog-caption">坐标将以 WGS-84 原始格式保存</div>
        </div>
      </q-card-section>

      <q-card-section v-if="location" class="q-pt-sm">
        <q-input
          v-model="placeName"
          outlined
          label="地点名称（可选）"
          maxlength="200"
          hint="系统识别可能不准确，可以在保存前修改"
        />

        <div class="location-facts">
          <div>
            <span>经纬度</span>
            <strong>{{ coordinates }}</strong>
          </div>
          <div>
            <span>水平精度</span>
            <strong>{{ accuracy }}</strong>
          </div>
          <div>
            <span>定位时间</span>
            <strong>{{ capturedTime }}</strong>
          </div>
        </div>
      </q-card-section>

      <q-card-actions align="right" class="location-dialog-actions">
        <q-btn flat label="取消" color="primary" @click="close"/>
        <q-btn flat label="重新定位" color="primary" @click="emit('retry')"/>
        <q-btn unelevated label="插入日记" color="primary" :disable="!location" @click="confirm"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<style scoped lang="scss">
.location-dialog-card {
  width: min(92vw, 520px);
  color: var(--pad-text-color);
  background: var(--pad-bg-color-200);
}

.location-dialog-heading {
  display: flex;
  align-items: center;
  gap: 12px;
}

.location-dialog-icon {
  display: grid;
  place-items: center;
  width: 42px;
  height: 42px;
  border-radius: 12px;
  color: var(--pad-primary-dark);
  background: color-mix(in srgb, var(--pad-primary-color) 16%, transparent);
}

.location-dialog-caption {
  margin-top: 2px;
  color: var(--pad-text-color-400);
  font-size: 13px;
}

.location-facts {
  display: grid;
  gap: 8px;
  margin-top: 20px;

  > div {
    display: grid;
    grid-template-columns: 76px minmax(0, 1fr);
    gap: 10px;
    padding: 9px 11px;
    border-radius: 8px;
    background: var(--pad-bg-color-300);
  }

  span {
    color: var(--pad-text-color-400);
  }

  strong {
    overflow-wrap: anywhere;
    color: var(--pad-text-color-200);
    font-weight: 500;
  }
}

.location-dialog-actions {
  padding: 8px 16px 14px;
}

:deep(.q-field__control) {
  background: var(--pad-bg-color-100);
}

:deep(.q-field__native),
:deep(.q-field__input) {
  color: var(--pad-text-color-200);
}

:deep(.q-field__label),
:deep(.q-field__marginal),
:deep(.q-field__bottom) {
  color: var(--pad-text-color-400);
}

:deep(.q-field--outlined .q-field__control::before) {
  border-color: var(--pad-border-color-100);
}
</style>
