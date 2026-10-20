<template>
  <q-btn
    ref="trigger"
    flat
    round
    dense
    size="sm"
    color="primary"
    icon="error_outline"
    class="details-help-button"
    :aria-label="title"
    :aria-expanded="opened"
    :aria-describedby="inDialog && opened ? contentId : undefined"
    :aria-haspopup="inDialog ? undefined : 'dialog'"
    @click.stop="opened = !opened"
  >
    <q-tooltip
      v-if="inDialog"
      :id="contentId"
      v-model="opened"
      no-parent-event
      :offset="[0, 8]"
      max-width="min(420px, calc(100vw - 32px))"
      max-height="min(60dvh, 520px)"
      class="details-help-tooltip"
    >
      <div ref="content" tabindex="0" class="details-help-content">
        <div class="details-help-title">{{ title }}</div>
        <slot />
      </div>
    </q-tooltip>
    <q-dialog v-else v-model="opened" :aria-labelledby="`${contentId}-title`">
      <q-card class="details-help-dialog">
        <q-card-section class="row items-center no-wrap q-gutter-sm">
          <div :id="`${contentId}-title`" class="text-h6 col">{{ title }}</div>
          <q-btn flat round dense icon="close" :aria-label="`关闭${title}`" v-close-popup />
        </q-card-section>
        <q-separator />
        <q-card-section class="details-help-dialog-body"><slot /></q-card-section>
      </q-card>
    </q-dialog>
  </q-btn>
</template>

<script setup lang="ts">
import {ref, useId} from 'vue';
import type {QBtn} from 'quasar';
import {onClickOutside, useEventListener} from '@vueuse/core';

const props = withDefaults(defineProps<{title: string; inDialog?: boolean}>(), {inDialog: false});
const opened = ref(false);
const trigger = ref<QBtn>();
const content = ref<HTMLElement>();
const contentId = `details-help-${useId()}`;

// QTooltip 的默认触摸行为会在松手时隐藏，长说明改为点击开关，并允许在浮层内滚动、复制。
onClickOutside(content, () => {
  if (props.inDialog) opened.value = false;
}, {ignore: [trigger]});

useEventListener(window, 'keydown', event => {
  if (props.inDialog && opened.value && event.key === 'Escape') {
    opened.value = false;
    // 不把本次 Escape 继续交给外层 QDialog，避免查看说明时意外关闭配置草稿。
    event.preventDefault();
    event.stopImmediatePropagation();
    trigger.value?.$el.focus();
  }
}, {capture: true});
</script>

<style scoped>
:global(.details-help-tooltip.q-tooltip) {
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-200);
  border: 1px solid var(--pad-border-color-200);
  border-radius: var(--pad-radius-md);
  box-shadow: var(--pad-shadow-md);
  padding: 14px 16px;
  font-size: 13px;
  line-height: 1.7;
  overflow-y: auto;
  overflow-wrap: anywhere;
  pointer-events: auto !important;
  user-select: text;
}
.details-help-title { color: var(--pad-text-color-100); font-weight: 600; margin-bottom: 8px; }
.details-help-content :deep(p), .details-help-dialog-body :deep(p) { margin: 0 0 10px; }
.details-help-content :deep(p:last-child), .details-help-dialog-body :deep(p:last-child) { margin-bottom: 0; }
.details-help-dialog {
  width: 480px;
  max-width: calc(100vw - 32px);
  max-height: calc(100dvh - 48px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-200);
  border-radius: var(--pad-radius-lg);
}
.details-help-dialog > :not(.details-help-dialog-body) { flex-shrink: 0; }
.details-help-dialog-body { min-height: 0; overflow-y: auto; line-height: 1.7; overflow-wrap: anywhere; }
</style>
