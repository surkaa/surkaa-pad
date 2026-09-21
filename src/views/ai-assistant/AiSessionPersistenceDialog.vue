<template>
  <q-dialog
    no-refocus
    :model-value="modelValue"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <q-card class="session-persistence-card">
      <q-card-section class="row items-center q-pb-sm">
        <div>
          <div class="text-h6">会话持久化详情</div>
          <div class="dialog-subtitle">显示解密后的原始会话数据，不包含模型上下文重建结果。</div>
        </div>
        <q-space/>
        <q-btn
          icon="close"
          flat
          round
          dense
          aria-label="关闭会话持久化详情"
          @click="emit('update:modelValue', false)"
        />
      </q-card-section>
      <q-separator/>

      <q-card-section v-if="loadingMeta" class="dialog-loading">
        <q-spinner color="primary" size="32px"/>
        <span>正在读取会话元数据</span>
      </q-card-section>
      <q-card-section v-else-if="meta" class="session-persistence-content">
        <section class="data-section">
          <div class="section-heading">
            <div>
              <h2>meta.enc</h2>
              <p>解密后的原始 JSON</p>
            </div>
          </div>
          <div class="json-panel">
            <JsonTreeViewer
              :source="meta"
              :expand-json-strings-by-default="false"
              expand-to-content
            />
          </div>
        </section>

        <section class="data-section message-section">
          <div class="section-heading">
            <div>
              <h2>消息记录</h2>
              <p>按保存顺序读取的原始消息 JSON</p>
            </div>
            <span class="message-count">已加载 {{ messages.length }} / {{ totalMessageCount }} 条</span>
          </div>

          <q-banner v-if="messageError" rounded class="message-error">
            <template #avatar><q-icon name="error_outline"/></template>
            <span>{{ messageError }}</span>
          </q-banner>
          <div v-else-if="loadingMessages && messages.length === 0" class="message-empty">
            <q-spinner color="primary" size="24px" />
            <span>正在读取消息</span>
          </div>
          <div v-else-if="messages.length === 0" class="message-empty">
            <q-icon name="chat_bubble_outline" size="26px"/>
            <span>该会话还没有保存消息</span>
          </div>
          <div v-else class="json-panel">
            <JsonTreeViewer
              :source="{messages}"
              :expand-json-strings-by-default="false"
              expand-to-content
            />
          </div>

          <q-btn
            v-if="hasMoreMessages && (messages.length > 0 || !loadingMessages)"
            outline
            no-caps
            color="primary"
            icon="expand_more"
            :label="`加载 ${nextMessageBatchSize} 条消息`"
            :loading="loadingMessages"
            :disable="loadingMessages"
            @click="emit('loadMore')"
          />
        </section>
      </q-card-section>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed} from 'vue';
import type {AiSessionMessage, AiSessionMeta} from '../../bindings';
import JsonTreeViewer from '../../components/JsonTreeViewer.vue';

const props = defineProps<{
  modelValue: boolean;
  meta: AiSessionMeta | null;
  messages: AiSessionMessage[];
  totalMessageCount: number;
  loadingMeta: boolean;
  loadingMessages: boolean;
  messageError: string | null;
  nextMessageBatchSize: number;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: boolean];
  loadMore: [];
}>();

const hasMoreMessages = computed(() => props.messages.length < props.totalMessageCount);
</script>

<style scoped lang="scss">
.session-persistence-card {
  display: flex;
  flex-direction: column;
  width: min(960px, 94vw);
  height: min(800px, 90vh);
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
}

.dialog-subtitle,
.section-heading p,
.message-count {
  color: var(--pad-text-color-400);
  font-size: 0.72rem;
}

.dialog-subtitle {
  margin-top: 2px;
}

.dialog-loading,
.message-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  color: var(--pad-text-color-400);
}

.dialog-loading {
  flex: 1;
  flex-direction: column;
}

.session-persistence-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 14px;
  background: var(--pad-bg-color-100);
}

.data-section + .data-section {
  margin-top: 18px;
}

.section-heading {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;

  h2,
  p {
    margin: 0;
  }

  h2 {
    color: var(--pad-text-color-200);
    font-size: 0.88rem;
    font-weight: 600;
  }
}

.json-panel {
  overflow: hidden;
  border: 1px solid var(--pad-border-color-100);
  border-radius: var(--pad-radius-md);
}

.message-error {
  color: var(--pad-danger-color);
  background: var(--pad-bg-color-200);
  border: 1px solid var(--pad-danger-color);
}

.message-empty {
  min-height: 108px;
  flex-direction: column;
  border: 1px dashed var(--pad-border-color-200);
  border-radius: var(--pad-radius-md);
  text-align: center;
}

.message-section > .q-btn {
  width: 100%;
  margin-top: 10px;
}

@media (max-width: 600px) {
  .session-persistence-card {
    width: 96vw;
    height: 92vh;
  }

  .session-persistence-content {
    padding: 10px;
  }

  .section-heading {
    align-items: flex-start;
    flex-direction: column;
    gap: 3px;
  }
}
</style>
