<template>
  <q-dialog
    no-refocus
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="conversation-source-card">
      <q-card-section class="row items-center q-pb-sm">
        <div class="text-h6">当前对话完整源码</div>
        <q-space/>
        <q-btn icon="close" flat round dense v-close-popup aria-label="关闭对话源码弹窗"/>
      </q-card-section>
      <q-separator/>
      <q-card-section class="conversation-source-content">
        <pre class="conversation-source">{{ sourceText }}</pre>
      </q-card-section>
      <q-separator/>
      <q-card-actions align="right">
        <q-btn
          flat
          icon="content_copy"
          label="复制完整源码"
          color="primary"
          :disable="!sourceText"
          @click="copySource"
        />
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed} from 'vue';
import {useQuasar} from 'quasar';
import type {AiConversationSource} from '../../bindings';
import {formatAiConversationSource} from '../../utils/aiAssistant';
import {copyTextToClipboard} from '../../utils/clipboard';
import {formatError} from '../../utils/formatError';

const props = defineProps<{
  modelValue: boolean;
  source: AiConversationSource | null;
}>();
const emit = defineEmits<{(event: 'update:modelValue', value: boolean): void}>();
const $q = useQuasar();
const sourceText = computed(() => props.source ? formatAiConversationSource(props.source) : '');

async function copySource() {
  if (!sourceText.value) return;
  try {
    await copyTextToClipboard(sourceText.value);
    $q.notify({type: 'positive', message: '当前对话完整源码已复制'});
  } catch (error) {
    $q.notify({type: 'negative', message: `复制对话源码失败：${formatError(error)}`});
  }
}
</script>

<style scoped lang="scss">
.conversation-source-card {
  display: flex;
  flex-direction: column;
  width: min(960px, 94vw);
  height: min(760px, 88vh);
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-100);
}

.conversation-source-content {
  flex: 1;
  min-height: 0;
  padding: 0;
}

.conversation-source {
  width: 100%;
  height: 100%;
  margin: 0;
  padding: 16px;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-200);
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
  line-height: 1.55;
}
</style>
