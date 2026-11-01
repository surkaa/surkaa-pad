<template>
  <!-- 等待动画的内部模型是满宽的，不能复用它向真实百分比做缩短动画。 -->
  <q-linear-progress
    :key="determinate ? 'determinate' : 'indeterminate'"
    :value="fraction"
    :indeterminate="!determinate"
    :animation-speed="200"
    color="primary"
  />
</template>

<script setup lang="ts">
import {computed} from 'vue';

const props = defineProps<{value?: number}>();
const determinate = computed(() => typeof props.value === 'number' && Number.isFinite(props.value));
const fraction = computed(() => determinate.value ? Math.max(0, Math.min(1, props.value!)) : 0);
</script>
