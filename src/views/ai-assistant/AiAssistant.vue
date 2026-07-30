<script setup lang="ts">
import {Channel} from '@tauri-apps/api/core';
import {computed, nextTick, onActivated, onBeforeUnmount, ref} from 'vue';
import {useRouter} from 'vue-router';
import {openUrl} from '@tauri-apps/plugin-opener';
import {useQuasar} from 'quasar';
import type {AiAgentEvent} from '../../bindings';
import {
  formatAiResponseMeta,
  initialAiAgentDisplayState,
  isTerminalAiExchangeState,
  reduceAiAgentEvent,
  startAiQuestion,
  type AiAgentDisplayState,
  type AiExchangeState,
} from '../../utils/aiAssistant';
import {renderAiMarkdown} from '../../utils/aiMarkdown';
import {
  loadAiServiceConfig,
  type AiServiceConfig,
} from '../../utils/aiConfig';
import api from '../../utils/api';
import {formatError} from '../../utils/formatError';

interface AiExchange extends AiAgentDisplayState {
  id: number;
  question: string;
  taskToken: string | null;
  cancelRequested: boolean;
}

const suggestions = [
  '总结最近三篇日记',
  '最近一篇日记写了什么？',
  '查找提到“旅行”的日记',
];

const router = useRouter();
const $q = useQuasar();
const scrollContainer = ref<HTMLElement | null>(null);
const config = ref<AiServiceConfig | null>(null);
const configError = ref<string | null>(null);
const loadingConfig = ref(true);
const question = ref('');
const sending = ref(false);
const exchanges = ref<AiExchange[]>([]);
const isCanceling = computed(() => exchanges.value.some(exchange => exchange.state === 'canceling'));
let nextExchangeId = 1;
let pendingScrollFrame: number | null = null;
let unmounting = false;

defineOptions({name: 'AiAssistant'});

onActivated(refreshConfig);
onBeforeUnmount(() => {
  unmounting = true;
  if (pendingScrollFrame !== null) cancelAnimationFrame(pendingScrollFrame);
  void cancelActiveQuestion(false);
});

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
    ...initialAiAgentDisplayState(),
    id: nextExchangeId++,
    question: prompt,
    taskToken: null,
    cancelRequested: false,
  };
  exchanges.value.push(exchange);
  question.value = '';
  sending.value = true;
  await scrollToBottom();

  const event = new Channel<AiAgentEvent>();
  event.onmessage = message => handleAgentEvent(exchange.id, message);

  try {
    const taskToken = await startAiQuestion(activeConfig, prompt, event);
    const current = findExchange(exchange.id);
    if (!current || isTerminalAiExchangeState(current.state)) return;
    current.taskToken = taskToken;
    if (current.cancelRequested) await cancelExchange(current, true);
  } catch (error) {
    const current = findExchange(exchange.id);
    if (current && !isTerminalAiExchangeState(current.state)) {
      finishExchange(current, 'failed', formatError(error));
    }
  }
}

function handleAgentEvent(id: number, message: AiAgentEvent) {
  const exchange = findExchange(id);
  if (!exchange || isTerminalAiExchangeState(exchange.state)) return;

  Object.assign(exchange, reduceAiAgentEvent(exchange, message));
  if (isTerminalAiExchangeState(exchange.state)) {
    finishExchange(exchange, exchange.state, exchange.error);
  }
  scheduleScrollToBottom();
}

async function cancelActiveQuestion(notify = true) {
  const exchange = [...exchanges.value]
    .reverse()
    .find(item => item.state === 'running' || item.state === 'canceling');
  if (!exchange) return;
  exchange.cancelRequested = true;
  exchange.state = 'canceling';
  exchange.status = '正在停止生成…';
  scheduleScrollToBottom();
  if (exchange.taskToken) await cancelExchange(exchange, notify);
}

async function cancelExchange(exchange: AiExchange, notify: boolean) {
  const taskToken = exchange.taskToken;
  if (!taskToken || isTerminalAiExchangeState(exchange.state)) return;
  try {
    await api.cmdCancelTask(taskToken);
  } catch (error) {
    if (!isTerminalAiExchangeState(exchange.state)) {
      exchange.state = 'running';
      exchange.cancelRequested = false;
      exchange.status = '正在生成回答…';
    }
    if (notify) {
      $q.notify({type: 'negative', message: `停止生成失败：${formatError(error)}`});
    }
  }
}

async function handleAnswerClick(event: MouseEvent) {
  const target = event.target instanceof Element ? event.target.closest('a') : null;
  if (!(target instanceof HTMLAnchorElement)) return;
  event.preventDefault();
  try {
    const url = new URL(target.href);
    if (!['http:', 'https:'].includes(url.protocol)) return;
    await openUrl(url.href);
  } catch (error) {
    $q.notify({type: 'negative', message: `打开链接失败：${formatError(error)}`});
  }
}

function findExchange(id: number) {
  return exchanges.value.find(exchange => exchange.id === id);
}

function finishExchange(exchange: AiExchange, state: Extract<AiExchangeState, 'completed' | 'failed' | 'cancelled'>, error: string | null = null) {
  exchange.state = state;
  exchange.error = error;
  exchange.taskToken = null;
  exchange.cancelRequested = false;
  sending.value = false;
  scheduleScrollToBottom();
}

function scheduleScrollToBottom() {
  if (unmounting || pendingScrollFrame !== null) return;
  pendingScrollFrame = requestAnimationFrame(() => {
    pendingScrollFrame = null;
    void scrollToBottom();
  });
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
            <div
              v-if="exchange.answer"
              :class="{'answer-text': true, 'ai-markdown': true, 'is-streaming': exchange.state === 'running'}"
              @click="handleAnswerClick"
              v-html="renderAiMarkdown(exchange.answer)"
            ></div>
            <div
              v-if="exchange.state === 'running' || exchange.state === 'canceling'"
              class="answer-status"
            >
              <q-spinner-dots v-if="exchange.state === 'running'" color="primary" size="24px"/>
              <q-spinner v-else color="primary" size="18px"/>
              <span>{{ exchange.status }}</span>
            </div>
            <template v-if="exchange.state === 'completed' && exchange.response">
              <div class="answer-meta">{{ formatAiResponseMeta(exchange.response) }}</div>
            </template>
            <div v-else-if="exchange.state === 'failed'" class="answer-error">
              <q-icon name="error_outline"/>
              <span>{{ exchange.error || 'AI 服务请求失败' }}</span>
            </div>
            <div v-else-if="exchange.state === 'cancelled'" class="answer-cancelled">
              <q-icon name="stop_circle"/>
              <span>已停止生成</span>
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
          :color="sending ? 'negative' : 'primary'"
          :icon="sending ? 'stop' : 'send'"
          :aria-label="sending ? '停止生成' : '发送问题'"
          :loading="isCanceling"
          :disable="sending ? isCanceling : !config || !question.trim()"
          @click="sending ? cancelActiveQuestion() : submitQuestion()"
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
  line-height: 1.65;

  &.is-streaming::after {
    content: '';
    display: inline-block;
    width: 2px;
    height: 1em;
    margin-left: 3px;
    vertical-align: -0.12em;
    background: var(--pad-primary-dark);
    animation: ai-cursor-blink 0.9s steps(1) infinite;
  }
}

.ai-markdown {
  :deep(> :first-child) {
    margin-top: 0;
  }

  :deep(> :last-child) {
    margin-bottom: 0;
  }

  :deep(h1),
  :deep(h2),
  :deep(h3),
  :deep(h4),
  :deep(h5),
  :deep(h6) {
    margin: 1.15em 0 0.55em;
    color: var(--pad-text-color-100);
    line-height: 1.35;
  }

  :deep(h1) { font-size: 1.45rem; }
  :deep(h2) { font-size: 1.3rem; }
  :deep(h3) { font-size: 1.16rem; }

  :deep(p),
  :deep(ul),
  :deep(ol),
  :deep(blockquote),
  :deep(pre),
  :deep(table) {
    margin: 0.7em 0;
  }

  :deep(ul),
  :deep(ol) {
    padding-left: 1.5em;
  }

  :deep(blockquote) {
    padding: 0.25em 0.85em;
    color: var(--pad-text-color-300);
    border-left: 3px solid var(--pad-border-color-300);
  }

  :deep(code) {
    padding: 0.12em 0.35em;
    border-radius: var(--pad-radius-sm);
    color: var(--pad-text-color-200);
    background: var(--pad-bg-color-300);
    font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  }

  :deep(pre) {
    overflow-x: auto;
    padding: 12px;
    border-radius: var(--pad-radius-md);
    background: var(--pad-bg-color-300);

    code {
      padding: 0;
      background: transparent;
    }
  }

  :deep(table) {
    display: block;
    max-width: 100%;
    overflow-x: auto;
    border-collapse: collapse;
  }

  :deep(th),
  :deep(td) {
    padding: 6px 10px;
    border: 1px solid var(--pad-border-color-200);
    text-align: left;
  }

  :deep(th) {
    background: var(--pad-bg-color-300);
  }

  :deep(a) {
    color: var(--pad-primary-dark);
    text-decoration: underline;
    cursor: pointer;
  }

  :deep(.ai-markdown-image-placeholder) {
    color: var(--pad-text-color-400);
  }
}

.answer-status,
.answer-error,
.answer-cancelled {
  display: flex;
  align-items: center;
  gap: 8px;
}

.answer-status,
.answer-meta {
  color: var(--pad-text-color-400);
}

.answer-status {
  margin-top: 6px;
  font-size: 0.8rem;
}

.answer-meta {
  margin-top: 10px;
  font-size: 0.75rem;
}

.answer-error {
  color: var(--pad-danger-color);
}

.answer-cancelled {
  color: var(--pad-text-color-400);
}

@keyframes ai-cursor-blink {
  50% { opacity: 0; }
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
