<template>
  <q-btn
    flat
    round
    dense
    color="primary"
    :icon="maximized ? 'fullscreen_exit' : 'fullscreen'"
    :aria-label="label"
    @click="emit('toggle')"
  >
    <q-tooltip>{{ label }}</q-tooltip>
  </q-btn>
</template>

<script setup lang="ts">
import {computed} from 'vue';

const props = defineProps<{maximized: boolean}>();
const emit = defineEmits<{toggle: []}>();
const label = computed(() => props.maximized ? '退出全屏' : '全屏查看');
</script>
