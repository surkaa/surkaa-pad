<script setup lang="ts">
import {Channel} from '@tauri-apps/api/core';
import {computed, nextTick, onActivated, onBeforeUnmount, ref} from 'vue';
import {useRouter} from 'vue-router';
import {openUrl} from '@tauri-apps/plugin-opener';
import {useQuasar} from 'quasar';
import type {AiAgentEvent, AiConversationSource} from '../../bindings';
import {
  formatAiResponseMeta,
  formatAiProcessSummary,
  formatProcessDuration,
  buildAiConversationHistory,
  initialAiAgentDisplayState,
  isTerminalAiExchangeState,
  nextAiProcessExpanded,
  reduceAiAgentEvent,
  startAiQuestion,
  type AiAgentDisplayState,
  type AiExchangeState,
  type AiProcessStep,
} from '../../utils/aiAssistant';
import {renderAiMarkdown} from '../../utils/aiMarkdown';
import {
  isAiModelAvailable,
  loadAiServiceConfig,
  type AiServiceConfig,
} from '../../utils/aiConfig';
import api from '../../utils/api';
import {formatError} from '../../utils/formatError';
import {useConfigStore} from '../../stores/config';
import {useAiAssistantShortcuts} from '../../composables/useAiAssistantShortcuts';
import AiConversationSourceDialog from './AiConversationSourceDialog.vue';

interface AiExchange extends AiAgentDisplayState {
  id: number;
  question: string;
  taskToken: string | null;
  cancelRequested: boolean;
  processExpanded: boolean;
}

type ModelCheckState = 'idle' | 'checking' | 'available' | 'unavailable' | 'failed';

const suggestions = [
  '总结最近三篇日记',
  '最近一篇日记写了什么？',
  '查找提到“旅行”的日记',
];

const router = useRouter();
const $q = useQuasar();
const appConfigStore = useConfigStore();
const shortcuts = appConfigStore.useTauriConfig('windows_ai_assistant_shortcuts');
const scrollContainer = ref<HTMLElement | null>(null);
const questionInput = ref<{focus: () => void} | null>(null);
const config = ref<AiServiceConfig | null>(null);
const configError = ref<string | null>(null);
const loadingConfig = ref(true);
const modelCheckState = ref<ModelCheckState>('idle');
const modelCheckError = ref<string | null>(null);
const question = ref('');
const sending = ref(false);
const exchanges = ref<AiExchange[]>([]);
const conversationSource = ref<AiConversationSource | null>(null);
const showConversationSource = ref(false);
const isCanceling = computed(() => exchanges.value.some(exchange => exchange.state === 'canceling'));
const modelReady = computed(() => !!config.value && modelCheckState.value === 'available');
const modelLabel = computed(() => {
  const model = config.value?.model;
  if (!model) return '';
  if (modelCheckState.value === 'checking') return `正在检查 ${model}…`;
  if (modelCheckState.value === 'unavailable') return `${model} · 当前不可用`;
  if (modelCheckState.value === 'failed') return `${model} · 无法验证`;
  return `${model} · 保留当前会话上下文`;
});
let nextExchangeId = 1;
let pendingScrollFrame: number | null = null;
let unmounting = false;
let configRefreshId = 0;

defineOptions({name: 'AiAssistant'});

useAiAssistantShortcuts(shortcuts, {
  focusInput: () => questionInput.value?.focus(),
});

onActivated(async () => {
  await refreshConfig();
  await nextTick();
  questionInput.value?.focus();
});
onBeforeUnmount(() => {
  unmounting = true;
  configRefreshId += 1;
  if (pendingScrollFrame !== null) cancelAnimationFrame(pendingScrollFrame);
  void cancelActiveQuestion(false);
});

async function refreshConfig() {
  const refreshId = ++configRefreshId;
  loadingConfig.value = true;
  configError.value = null;
  modelCheckState.value = 'idle';
  modelCheckError.value = null;
  try {
    const loadedConfig = await loadAiServiceConfig();
    if (refreshId !== configRefreshId) return;
    config.value = loadedConfig;
    loadingConfig.value = false;
    if (loadedConfig) await checkModelAvailability(loadedConfig, refreshId);
  } catch (error) {
    if (refreshId !== configRefreshId) return;
    config.value = null;
    configError.value = formatError(error);
  } finally {
    if (refreshId === configRefreshId) loadingConfig.value = false;
  }
}

async function checkModelAvailability(
  activeConfig: AiServiceConfig,
  refreshId = ++configRefreshId,
) {
  modelCheckState.value = 'checking';
  modelCheckError.value = null;
  try {
    const available = await isAiModelAvailable(activeConfig);
    if (refreshId !== configRefreshId) return;
    modelCheckState.value = available ? 'available' : 'unavailable';
  } catch (error) {
    if (refreshId !== configRefreshId) return;
    modelCheckState.value = 'failed';
    modelCheckError.value = formatError(error);
  }
}

function retryModelCheck() {
  const activeConfig = config.value;
  if (activeConfig) void checkModelAvailability(activeConfig);
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
  if (!activeConfig || !modelReady.value || !prompt || sending.value) return;
  const history = buildAiConversationHistory(exchanges.value);

  const exchange: AiExchange = {
    ...initialAiAgentDisplayState(),
    id: nextExchangeId++,
    question: prompt,
    taskToken: null,
    cancelRequested: false,
    processExpanded: true,
  };
  exchanges.value.push(exchange);
  question.value = '';
  sending.value = true;
  await scrollToBottom();

  const event = new Channel<AiAgentEvent>();
  event.onmessage = message => handleAgentEvent(exchange.id, message);

  try {
    const taskToken = await startAiQuestion(activeConfig, prompt, history, event);
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
  if (!exchange) return;
  if (message.event === 'conversationSource') {
    conversationSource.value = message.data;
    return;
  }
  if (isTerminalAiExchangeState(exchange.state)) return;

  const processExpanded = nextAiProcessExpanded(exchange.processExpanded, exchange, message);
  Object.assign(exchange, reduceAiAgentEvent(exchange, message));
  exchange.processExpanded = processExpanded;
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
  exchange.processExpanded = state !== 'completed';
  sending.value = false;
  scheduleScrollToBottom();
}

function processHeader(exchange: AiExchange): string {
  if (exchange.state === 'running' || exchange.state === 'canceling') {
    return exchange.status || 'AI 正在处理…';
  }
  const summary = formatAiProcessSummary(exchange.processSteps);
  if (exchange.state === 'failed') return `${summary} · 处理失败`;
  if (exchange.state === 'cancelled') return `${summary} · 已停止`;
  return summary;
}

function processStepIcon(step: AiProcessStep): string {
  if (step.state === 'failed') return 'error';
  if (step.state === 'cancelled') return 'stop_circle';
  return 'check_circle';
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

        <div v-if="loadingConfig || modelCheckState === 'checking'" class="config-loading">
          <q-spinner-dots color="primary" size="32px"/>
          <span>{{ loadingConfig ? '正在读取 AI 配置' : '正在检查所选模型' }}</span>
        </div>
        <q-banner v-else-if="!config" rounded class="config-banner">
          <div>{{ configError ? `读取 AI 配置失败：${configError}` : '尚未配置 AI 服务' }}</div>
          <template #action>
            <q-btn flat color="primary" label="前往设置" @click="openSettings"/>
          </template>
        </q-banner>
        <q-banner v-else-if="modelCheckState === 'unavailable'" rounded class="config-banner">
          <div>模型“{{ config.model }}”已不在服务提供的模型列表中，请重新选择。</div>
          <template #action>
            <q-btn flat color="primary" label="前往设置" @click="openSettings"/>
          </template>
        </q-banner>
        <q-banner v-else-if="modelCheckState === 'failed'" rounded class="config-banner">
          <div>检查模型失败：{{ modelCheckError }}</div>
          <template #action>
            <q-btn flat color="primary" label="重新检查" @click="retryModelCheck"/>
          </template>
        </q-banner>
        <div v-else-if="modelReady" class="suggestions">
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
            <div v-if="exchange.processSteps.length" class="process-panel">
              <button
                type="button"
                class="process-header"
                :aria-expanded="exchange.processExpanded"
                @click="exchange.processExpanded = !exchange.processExpanded"
              >
                <q-spinner-dots
                  v-if="exchange.state === 'running'"
                  color="primary"
                  size="20px"
                />
                <q-spinner
                  v-else-if="exchange.state === 'canceling'"
                  color="primary"
                  size="16px"
                />
                <q-icon v-else name="account_tree" class="process-header-icon"/>
                <span>{{ processHeader(exchange) }}</span>
                <q-icon
                  name="expand_more"
                  class="process-expand-icon"
                  :class="{'is-expanded': exchange.processExpanded}"
                />
              </button>
              <div
                class="process-collapse"
                :class="{'is-expanded': exchange.processExpanded}"
                :aria-hidden="!exchange.processExpanded"
              >
                <div class="process-collapse-inner">
                  <div class="process-steps">
                    <div
                      v-for="step in exchange.processSteps"
                      :key="step.id"
                      class="process-step"
                      :class="`is-${step.state}`"
                    >
                      <div class="process-step-marker">
                        <q-spinner
                          v-if="step.state === 'running'"
                          color="primary"
                          size="15px"
                        />
                        <q-icon v-else :name="processStepIcon(step)"/>
                      </div>
                      <div class="process-step-content">
                        <div class="process-step-title">{{ step.title }}</div>
                        <div v-if="step.detail" class="process-step-detail">{{ step.detail }}</div>
                        <div
                          v-if="step.reasoning"
                          class="reasoning-content ai-markdown"
                          v-html="renderAiMarkdown(step.reasoning)"
                        ></div>
                      </div>
                      <div v-if="step.durationMs !== null" class="process-step-duration">
                        {{ formatProcessDuration(step.durationMs) }}
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
            <div
              v-if="exchange.answer"
              :class="{'answer-text': true, 'ai-markdown': true, 'is-streaming': exchange.state === 'running'}"
              @click="handleAnswerClick"
              v-html="renderAiMarkdown(exchange.answer)"
            ></div>
            <div
              v-if="(exchange.state === 'running' || exchange.state === 'canceling') && exchange.processSteps.length === 0"
              class="answer-status"
            >
              <q-spinner-dots v-if="exchange.state === 'running'" color="primary" size="24px"/>
              <q-spinner v-else color="primary" size="18px"/>
              <span>{{ exchange.status }}</span>
            </div>
            <template v-if="exchange.state === 'completed' && exchange.response">
              <div v-if="exchange.response.usage" class="answer-meta">
                {{ formatAiResponseMeta(exchange.response) }}
              </div>
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
        <q-spinner v-if="modelCheckState === 'checking'" color="primary" size="13px"/>
        <q-icon
          v-else-if="modelCheckState === 'unavailable' || modelCheckState === 'failed'"
          name="warning_amber"
        />
        <span>{{ modelLabel }}</span>
        <q-btn
          v-if="conversationSource"
          flat
          dense
          no-caps
          icon="data_object"
          label="源码"
          :disable="sending"
          aria-label="查看当前对话完整源码"
          @click="showConversationSource = true"
        />
        <q-btn
          v-if="modelCheckState === 'failed'"
          flat
          dense
          no-caps
          color="primary"
          label="重试"
          @click="retryModelCheck"
        />
        <q-btn
          v-else-if="modelCheckState === 'unavailable'"
          flat
          dense
          no-caps
          color="primary"
          label="重新选择"
          @click="openSettings"
        />
      </div>
      <div class="composer-row">
        <q-input
          ref="questionInput"
          v-model="question"
          type="textarea"
          autogrow
          outlined
          dense
          rows="1"
          maxlength="4000"
          :disable="!modelReady || sending"
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
          :disable="sending ? isCanceling : !modelReady || !question.trim()"
          @click="sending ? cancelActiveQuestion() : submitQuestion()"
        />
      </div>
      <div class="privacy-hint">当前会话历史、问题及 Agent 读取的日记文字会发送到你配置的 AI 服务</div>
    </div>
    <AiConversationSourceDialog
      v-model="showConversationSource"
      :source="conversationSource"
    />
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

.process-panel {
  margin-bottom: 10px;
  overflow: hidden;
  border: 1px solid var(--pad-border-color-100);
  border-radius: var(--pad-radius-md);
  background: var(--pad-bg-color-100);
}

.process-header {
  width: 100%;
  min-height: 38px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border: 0;
  color: var(--pad-text-color-300);
  background: transparent;
  font: inherit;
  font-size: 0.78rem;
  text-align: left;
  cursor: pointer;

  span {
    flex: 1;
  }
}

.process-header-icon {
  color: var(--pad-primary-dark);
}

.process-expand-icon {
  flex: none;
  color: var(--pad-text-color-400);
  transition: transform 0.2s ease;

  &.is-expanded {
    transform: rotate(180deg);
  }
}

.process-collapse {
  display: grid;
  grid-template-rows: 0fr;
  transition: grid-template-rows 0.2s cubic-bezier(.25, .8, .5, 1);

  &.is-expanded {
    grid-template-rows: 1fr;
  }
}

.process-collapse-inner {
  min-height: 0;
  overflow: hidden;
}

.process-steps {
  padding: 2px 10px 9px;
  border-top: 1px solid var(--pad-border-color-100);
}

.process-step {
  min-height: 38px;
  display: flex;
  align-items: flex-start;
  gap: 9px;
  padding: 8px 0;

  & + & {
    border-top: 1px solid var(--pad-border-color-100);
  }

  &.is-failed .process-step-marker {
    color: var(--pad-danger-color);
  }

  &.is-cancelled .process-step-marker {
    color: var(--pad-text-color-400);
  }
}

.process-step-marker {
  flex: none;
  width: 18px;
  height: 20px;
  display: grid;
  place-items: center;
  color: var(--pad-primary-dark);
  font-size: 16px;
}

.process-step-content {
  min-width: 0;
  flex: 1;
}

.process-step-title {
  color: var(--pad-text-color-200);
  font-size: 0.78rem;
  line-height: 1.35;
}

.process-step-detail,
.process-step-duration {
  color: var(--pad-text-color-400);
  font-size: 0.7rem;
  line-height: 1.35;
}

.process-step-detail {
  margin-top: 2px;
  overflow-wrap: anywhere;
}

.reasoning-content {
  margin-top: 7px;
  padding-left: 9px;
  color: var(--pad-text-color-300);
  border-left: 2px solid var(--pad-border-color-200);
  font-size: 0.72rem;
  line-height: 1.55;
}

.process-step-duration {
  flex: none;
  padding-top: 1px;
  white-space: nowrap;
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

@media (prefers-reduced-motion: reduce) {
  .process-collapse,
  .process-expand-icon {
    transition: none;
  }
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
  min-height: 28px;
  display: flex;
  align-items: center;
  gap: 5px;
  margin-bottom: 5px;
  text-align: left;

  span {
    flex: 1;
  }
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
