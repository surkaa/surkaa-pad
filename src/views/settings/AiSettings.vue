<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">AI 助手</div>
    <q-list bordered class="pad-card">
      <q-item clickable v-ripple class="settings-item" @click="openConfigDialog">
        <q-item-section avatar class="settings-icon-section">
          <q-icon name="auto_awesome"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium">AI 服务</q-item-label>
          <q-item-label caption class="desc-text">{{ configSummary }}</q-item-label>
        </q-item-section>
        <q-item-section side>
          <q-icon name="chevron_right" class="desc-text"/>
        </q-item-section>
      </q-item>
    </q-list>

    <q-dialog v-model="showDialog">
      <q-card class="ai-config-modal">
        <q-card-section>
          <div class="text-h6 title-text">配置 AI 服务</div>
          <div class="text-caption desc-text">
            支持提供模型列表和 Chat Completions 的 OpenAI 兼容接口
          </div>
        </q-card-section>

        <q-card-section class="q-pt-none q-gutter-y-sm">
          <q-banner rounded class="privacy-notice">
            问题以及 Agent 按需读取的日记文本会发送到此服务，请确认你信任服务提供方。
          </q-banner>

          <q-input
            v-model="draft.baseUrl"
            label="API 地址"
            hint="例如 http://localhost:11434/v1"
            outlined
            dense
            color="primary"
            @update:model-value="resetDiscoveredModels"
          />
          <q-input
            v-model="draft.apiKey"
            :type="showApiKey ? 'text' : 'password'"
            label="API Key（可选）"
            outlined
            dense
            color="primary"
            @update:model-value="resetDiscoveredModels"
          >
            <template #append>
              <q-icon
                :name="showApiKey ? 'visibility_off' : 'visibility'"
                class="cursor-pointer"
                @click="showApiKey = !showApiKey"
              />
            </template>
          </q-input>

          <q-banner v-if="endpointSecurity === 'remoteHttp'" rounded class="http-warning">
            远程 HTTP 连接不会加密传输内容，建议改用 HTTPS。
          </q-banner>
          <q-banner v-else-if="endpointSecurity === 'localHttp'" rounded class="local-http-hint">
            本机 Ollama 通常使用 HTTP；如果地址指向其他设备，建议改用 HTTPS。
          </q-banner>

          <div class="row items-center q-gutter-sm no-wrap">
            <q-btn
              outline
              no-caps
              color="primary"
              label="获取模型"
              :loading="loadingModels"
              @click="loadModels"
            />
            <q-select
              v-model="draft.model"
              :options="modelOptions"
              label="模型"
              outlined
              dense
              emit-value
              map-options
              options-dense
              popup-content-class="pad-ai-model-menu"
              class="col model-select"
              :disable="modelOptions.length === 0"
            />
          </div>
        </q-card-section>

        <q-card-actions align="between" class="q-px-md q-pb-md">
          <q-btn
            v-if="savedConfig"
            flat
            label="清除配置"
            color="negative"
            :disable="saving"
            @click="clearConfig"
          />
          <q-space v-else/>
          <div class="row q-gutter-sm">
            <q-btn flat label="取消" class="desc-text" v-close-popup :disable="saving"/>
            <q-btn
              unelevated
              label="保存"
              color="primary"
              :loading="saving"
              :disable="!draft.model"
              @click="saveConfig"
            />
          </div>
        </q-card-actions>
      </q-card>
    </q-dialog>
  </section>
</template>

<script setup lang="ts">
import {computed, onMounted, reactive, ref} from 'vue';
import {useQuasar} from 'quasar';
import type {AiModel} from '../../bindings';
import api from '../../utils/api';
import {
  classifyAiEndpoint,
  clearAiServiceConfig,
  DEFAULT_AI_BASE_URL,
  loadAiServiceConfig,
  saveAiServiceConfig,
  type AiServiceConfig,
} from '../../utils/aiConfig';
import {formatError} from '../../utils/formatError';

const $q = useQuasar();
const showDialog = ref(false);
const showApiKey = ref(false);
const loadingModels = ref(false);
const saving = ref(false);
const savedConfig = ref<AiServiceConfig | null>(null);
const models = ref<AiModel[]>([]);
const draft = reactive<AiServiceConfig>({
  baseUrl: DEFAULT_AI_BASE_URL,
  apiKey: '',
  model: '',
});

const configSummary = computed(() => {
  if (!savedConfig.value) return '未配置';
  try {
    return `${savedConfig.value.model} · ${new URL(savedConfig.value.baseUrl).host}`;
  } catch {
    return savedConfig.value.model;
  }
});
const endpointSecurity = computed(() => classifyAiEndpoint(draft.baseUrl));
const modelOptions = computed(() => models.value.map(model => ({
  label: model.id,
  value: model.id,
  caption: model.ownedBy || undefined,
})));

onMounted(refreshConfig);

async function refreshConfig() {
  try {
    savedConfig.value = await loadAiServiceConfig();
  } catch (error) {
    savedConfig.value = null;
    $q.notify({type: 'negative', message: `读取 AI 配置失败: ${formatError(error)}`});
  }
}

function openConfigDialog() {
  const config = savedConfig.value;
  draft.baseUrl = config?.baseUrl || DEFAULT_AI_BASE_URL;
  draft.apiKey = config?.apiKey || '';
  draft.model = config?.model || '';
  models.value = config ? [{id: config.model, ownedBy: null}] : [];
  showApiKey.value = false;
  showDialog.value = true;
}

function resetDiscoveredModels() {
  models.value = [];
  draft.model = '';
}

async function loadModels() {
  if (!draft.baseUrl.trim()) {
    $q.notify({type: 'warning', message: '请填写 API 地址'});
    return;
  }
  loadingModels.value = true;
  try {
    const result = await api.cmdListAiModels(
      draft.baseUrl,
      draft.apiKey.trim() || null,
    );
    models.value = result;
    if (result.length === 0) {
      draft.model = '';
      $q.notify({
        type: 'warning',
        message: '服务中暂无可用模型；如使用 Ollama，请先下载一个模型',
      });
      return;
    }
    if (!result.some(model => model.id === draft.model)) {
      draft.model = result[0].id;
    }
    $q.notify({type: 'positive', message: `已获取 ${result.length} 个模型`});
  } catch (error) {
    models.value = [];
    draft.model = '';
    $q.notify({type: 'negative', message: `连接 AI 服务失败: ${formatError(error)}`});
  } finally {
    loadingModels.value = false;
  }
}

async function saveConfig() {
  saving.value = true;
  try {
    savedConfig.value = await saveAiServiceConfig({...draft});
    showDialog.value = false;
    $q.notify({type: 'positive', message: 'AI 服务配置已保存'});
  } catch (error) {
    $q.notify({type: 'negative', message: `保存 AI 配置失败: ${formatError(error)}`});
  } finally {
    saving.value = false;
  }
}

async function clearConfig() {
  saving.value = true;
  try {
    await clearAiServiceConfig();
    savedConfig.value = null;
    showDialog.value = false;
    $q.notify({type: 'positive', message: 'AI 服务配置已清除'});
  } catch (error) {
    $q.notify({type: 'negative', message: `清除 AI 配置失败: ${formatError(error)}`});
  } finally {
    saving.value = false;
  }
}
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>

<style scoped lang="scss">
.ai-config-modal {
  width: min(520px, calc(100vw - 32px));
  max-width: 520px;
  background-color: var(--pad-bg-color-100);
  color: var(--pad-text-color-100);
  border-radius: var(--pad-radius-xl);

  .title-text {
    color: var(--pad-text-color-100);
  }

  .desc-text {
    color: var(--pad-text-color-400);
  }

  .privacy-notice,
  .local-http-hint {
    color: var(--pad-text-color-300);
    background: var(--pad-bg-color-300);
  }

  .http-warning {
    color: var(--pad-warning-dark);
    background: color-mix(in srgb, var(--pad-warning-color) 16%, transparent);
  }

  :deep(.q-field__control) {
    background-color: var(--pad-bg-color-200);
  }

  :deep(.q-field__native),
  :deep(.q-field__input),
  :deep(.q-field__label) {
    color: var(--pad-text-color-200) !important;
  }

  :deep(.q-field__marginal),
  :deep(.q-field__bottom) {
    color: var(--pad-text-color-400);
  }

  :deep(.q-field--outlined .q-field__control::before) {
    border-color: var(--pad-border-color-100);
  }
}

.model-select {
  min-width: 0;
}
</style>
