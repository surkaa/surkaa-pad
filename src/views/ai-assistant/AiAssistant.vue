<script setup lang="ts">
import {nextTick, onActivated, ref} from 'vue';
import {useRouter} from 'vue-router';
import type {AiAgentResponse} from '../../bindings';
import {formatAiResponseMeta, runAiQuestion} from '../../utils/aiAssistant';
import {
  loadAiServiceConfig,
  type AiServiceConfig,
} from '../../utils/aiConfig';
import {formatError} from '../../utils/formatError';

type ExchangeState = 'loading' | 'completed' | 'failed';

interface AiExchange {
  id: number;
  question: string;
  state: ExchangeState;
  response: AiAgentResponse | null;
  error: string | null;
}

const suggestions = [
  '总结最近三篇日记',
  '最近一篇日记写了什么？',
  '查找提到“旅行”的日记',
];

const router = useRouter();
const scrollContainer = ref<HTMLElement | null>(null);
const config = ref<AiServiceConfig | null>(null);
const configError = ref<string | null>(null);
const loadingConfig = ref(true);
const question = ref('');
const sending = ref(false);
const exchanges = ref<AiExchange[]>([]);
let nextExchangeId = 1;

defineOptions({name: 'AiAssistant'});

onActivated(refreshConfig);

async function refreshConfig() {
  loadingConfig.value = true;
  configError.value = null;
  try {
    config.value = await loadAiServiceConfig();
  } catch (error) {
    config.value = null;
    configError.value = formatError(error);
  } finally {
    loadingConfig.value = false;
  }
}

function openSettings() {
  void router.push({name: 'Settings'});
}

function applySuggestion(suggestion: string) {
  question.value = suggestion;
}

function handleComposerKeydown(event: KeyboardEvent) {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  void submitQuestion();
}

async function submitQuestion() {
  const activeConfig = config.value;
  const prompt = question.value.trim();
  if (!activeConfig || !prompt || sending.value) return;

  const exchange: AiExchange = {
    id: nextExchangeId++,
    question: prompt,
    state: 'loading',
    response: null,
    error: null,
  };
  exchanges.value.push(exchange);
  question.value = '';
  sending.value = true;
  await scrollToBottom();

  try {
    const response = await runAiQuestion(activeConfig, prompt);
    replaceExchange(exchange.id, {
      ...exchange,
      state: 'completed',
      response,
    });
  } catch (error) {
    replaceExchange(exchange.id, {
      ...exchange,
      state: 'failed',
      error: formatError(error),
    });
  } finally {
    sending.value = false;
    await scrollToBottom();
  }
}

function replaceExchange(id: number, replacement: AiExchange) {
  const index = exchanges.value.findIndex(exchange => exchange.id === id);
  if (index !== -1) exchanges.value[index] = replacement;
}

async function scrollToBottom() {
  await nextTick();
  const container = scrollContainer.value;
  if (container) container.scrollTop = container.scrollHeight;
}
</script>

<template>
  <div id="ai-assistant">
    <section ref="scrollContainer" class="conversation" aria-live="polite">
      <div v-if="exchanges.length === 0" class="welcome-panel">
        <q-icon name="auto_awesome" size="44px" class="welcome-icon"/>
        <h1>问问你的日记</h1>
        <p>AI Agent 可以按需搜索和读取日记文字，但不会修改任何内容，也无法理解图片或播放音视频。</p>

        <div v-if="loadingConfig" class="config-loading">
          <q-spinner-dots color="primary" size="32px"/>
          <span>正在读取 AI 配置</span>
        </div>
        <q-banner v-else-if="!config" rounded class="config-banner">
          <div>{{ configError ? `读取 AI 配置失败：${configError}` : '尚未配置 AI 服务' }}</div>
          <template #action>
            <q-btn flat color="primary" label="前往设置" @click="openSettings"/>
          </template>
        </q-banner>
        <div v-else class="suggestions">
          <div class="suggestion-title">可以试着问</div>
          <q-btn
            v-for="suggestion in suggestions"
            :key="suggestion"
            outline
            no-caps
            color="primary"
            :label="suggestion"
            @click="applySuggestion(suggestion)"
          />
        </div>
      </div>

      <article v-for="exchange in exchanges" :key="exchange.id" class="exchange">
        <div class="question-row">
          <div class="message question-message">{{ exchange.question }}</div>
        </div>
        <div class="answer-row">
          <div class="assistant-avatar">
            <q-icon name="auto_awesome" size="18px"/>
          </div>
          <div class="message answer-message">
            <div v-if="exchange.state === 'loading'" class="answer-loading">
              <q-spinner-dots color="primary" size="24px"/>
              <span>正在查找并读取相关日记…</span>
            </div>
            <template v-else-if="exchange.response">
              <div class="answer-text">{{ exchange.response.answer }}</div>
              <div class="answer-meta">{{ formatAiResponseMeta(exchange.response) }}</div>
            </template>
            <div v-else class="answer-error">
              <q-icon name="error_outline"/>
              <span>{{ exchange.error || 'AI 服务请求失败' }}</span>
            </div>
          </div>
        </div>
      </article>
    </section>

    <div class="composer-area">
      <div v-if="config" class="model-label">
        {{ config.model }} · 每次提问独立处理
      </div>
      <div class="composer-row">
        <q-input
          v-model="question"
          type="textarea"
          autogrow
          outlined
          dense
          rows="1"
          maxlength="4000"
          :disable="!config || loadingConfig || sending"
          placeholder="输入想从日记中了解的问题"
          aria-label="向 AI 助手提问"
          class="question-input"
          @keydown="handleComposerKeydown"
        />
        <q-btn
          round
          unelevated
          color="primary"
          icon="send"
          aria-label="发送问题"
          :loading="sending"
          :disable="!config || !question.trim() || sending"
          @click="submitQuestion"
        />
      </div>
      <div class="privacy-hint">问题及 Agent 读取的日记文字会发送到你配置的 AI 服务</div>
    </div>
  </div>
</template>

<style scoped lang="scss">
#ai-assistant {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-100);
}

.conversation {
  flex: 1;
  overflow-y: auto;
  padding: 28px max(20px, calc((100% - 820px) / 2)) 32px;
}

.welcome-panel {
  max-width: 680px;
  margin: min(10vh, 72px) auto 0;
  text-align: center;

  h1 {
    margin: 14px 0 8px;
    font-size: clamp(1.5rem, 4vw, 2rem);
    color: var(--pad-text-color-100);
  }

  p {
    max-width: 580px;
    margin: 0 auto;
    color: var(--pad-text-color-300);
  }
}

.welcome-icon {
  color: var(--pad-primary-dark);
}

.config-loading {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 10px;
  margin-top: 28px;
  color: var(--pad-text-color-400);
}

.config-banner {
  max-width: 520px;
  margin: 28px auto 0;
  text-align: left;
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
  border: 1px solid var(--pad-border-color-100);
}

.suggestions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 10px;
  margin-top: 30px;
}

.suggestion-title {
  width: 100%;
  margin-bottom: 2px;
  color: var(--pad-text-color-400);
  font-size: 0.82rem;
}

.exchange + .exchange {
  margin-top: 28px;
}

.question-row,
.answer-row {
  display: flex;
}

.question-row {
  justify-content: flex-end;
  margin-bottom: 12px;
}

.answer-row {
  align-items: flex-start;
  gap: 10px;
}

.message {
  max-width: min(88%, 680px);
  padding: 12px 15px;
  border-radius: var(--pad-radius-lg);
  text-align: left;
  overflow-wrap: anywhere;
}

.question-message {
  color: var(--pad-on-primary-color);
  background: var(--pad-primary-color);
  border-bottom-right-radius: var(--pad-radius-sm);
  white-space: pre-wrap;
}

.assistant-avatar {
  flex: none;
  width: 32px;
  height: 32px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  color: var(--pad-primary-dark);
  background: var(--pad-bg-color-300);
}

.answer-message {
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
  border: 1px solid var(--pad-border-color-100);
  border-top-left-radius: var(--pad-radius-sm);
}

.answer-text {
  white-space: pre-wrap;
}

.answer-loading,
.answer-error {
  display: flex;
  align-items: center;
  gap: 8px;
}

.answer-loading,
.answer-meta {
  color: var(--pad-text-color-400);
}

.answer-meta {
  margin-top: 10px;
  font-size: 0.75rem;
}

.answer-error {
  color: var(--pad-danger-color);
}

.composer-area {
  flex: none;
  padding: 10px max(16px, calc((100% - 820px) / 2)) 12px;
  background: var(--pad-bg-color-200);
  border-top: 1px solid var(--pad-border-color-100);
}

.composer-row {
  display: flex;
  align-items: flex-end;
  gap: 10px;
}

.question-input {
  flex: 1;

  :deep(.q-field__control) {
    max-height: 140px;
    overflow-y: auto;
    background: var(--pad-bg-color-100);
  }

  :deep(.q-field__native),
  :deep(.q-field__input) {
    color: var(--pad-text-color-200);
  }

  :deep(.q-field--outlined .q-field__control::before) {
    border-color: var(--pad-border-color-200);
  }
}

.model-label,
.privacy-hint {
  color: var(--pad-text-color-400);
  font-size: 0.72rem;
}

.model-label {
  margin-bottom: 5px;
  text-align: left;
}

.privacy-hint {
  margin-top: 6px;
  text-align: center;
}

@media (max-width: 512px) {
  .conversation {
    padding: 20px 12px 24px;
  }

  .welcome-panel {
    margin-top: 28px;
  }

  .message {
    max-width: calc(100% - 42px);
  }

  .composer-area {
    padding-inline: 10px;
  }
}
</style>
