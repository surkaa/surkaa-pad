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

          <div class="row items-center q-gutter-sm no-wrap">
            <q-select
              :model-value="selectedProfileId"
              :options="profileOptions"
              label="当前环境"
              outlined
              dense
              emit-value
              map-options
              options-dense
              :dark="$q.dark.isActive"
              :options-dark="$q.dark.isActive"
              popup-content-class="settings-select-popup"
              class="col profile-select"
              @update:model-value="switchProfile"
            />
            <q-btn outline color="primary" icon="add" aria-label="新增 AI 环境" @click="addProfile"/>
            <q-btn
              outline
              color="negative"
              icon="delete"
              aria-label="删除当前 AI 环境"
              :disable="profiles.length <= 1"
              @click="deleteProfile"
            />
          </div>

          <q-input
            v-model="draft.name"
            label="环境名称"
            outlined
            dense
            color="primary"
          />

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
              :dark="$q.dark.isActive"
              :options-dark="$q.dark.isActive"
              popup-content-class="settings-select-popup"
              class="col model-select"
              :disable="modelOptions.length === 0"
            />
          </div>
        </q-card-section>

        <q-card-actions align="between" class="q-px-md q-pb-md">
          <q-btn
            v-if="savedSet"
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
  loadAiServiceConfigSet,
  normalizeAiServiceConfigSet,
  normalizeAiServiceProfile,
  saveAiServiceConfigSet,
  type AiServiceConfigSet,
  type AiServiceProfile,
} from '../../utils/aiConfig';
import {formatError} from '../../utils/formatError';

const $q = useQuasar();
const showDialog = ref(false);
const showApiKey = ref(false);
const loadingModels = ref(false);
const saving = ref(false);
const savedSet = ref<AiServiceConfigSet | null>(null);
const profiles = ref<AiServiceProfile[]>([]);
const selectedProfileId = ref<string | null>(null);
const models = ref<AiModel[]>([]);
const draft = reactive<AiServiceProfile>({
  id: 'default',
  name: '默认环境',
  baseUrl: DEFAULT_AI_BASE_URL,
  apiKey: '',
  model: '',
  models: [],
});

const configSummary = computed(() => {
  const savedProfile = savedSet.value?.profiles.find(
    profile => profile.id === savedSet.value?.activeProfileId,
  );
  if (!savedProfile) return '未配置';
  try {
    return `${savedProfile.name} · ${savedProfile.model} · ${new URL(savedProfile.baseUrl).host}`;
  } catch {
    return `${savedProfile.name} · ${savedProfile.model}`;
  }
});
const endpointSecurity = computed(() => classifyAiEndpoint(draft.baseUrl));
const profileOptions = computed(() => profiles.value.map(profile => ({
  label: profile.name,
  value: profile.id,
})));
const modelOptions = computed(() => models.value.map(model => ({
  label: model.id,
  value: model.id,
  caption: model.ownedBy || undefined,
})));

onMounted(refreshConfig);

async function refreshConfig() {
  try {
    savedSet.value = await loadAiServiceConfigSet();
  } catch (error) {
    savedSet.value = null;
    $q.notify({type: 'negative', message: `读取 AI 配置失败: ${formatError(error)}`});
  }
}

function emptyProfile(id = 'default', name = '默认环境'): AiServiceProfile {
  return {
    id,
    name,
    baseUrl: DEFAULT_AI_BASE_URL,
    apiKey: '',
    model: '',
    models: [],
  };
}

function copyProfile(profile: AiServiceProfile): AiServiceProfile {
  return {...profile, models: profile.models.map(model => ({...model}))};
}

function loadDraft(profile: AiServiceProfile) {
  Object.assign(draft, copyProfile(profile));
  models.value = draft.models.map(model => ({...model}));
}

function commitDraft() {
  if (!selectedProfileId.value) return;
  const index = profiles.value.findIndex(profile => profile.id === selectedProfileId.value);
  if (index === -1) return;
  try {
    profiles.value[index] = normalizeAiServiceProfile(draft, draft.id, draft.name);
  } catch {
    // 保存时再展示配置错误；切换环境不应破坏当前已保存的草稿。
  }
}

function openConfigDialog() {
  const set = savedSet.value;
  profiles.value = set?.profiles.map(copyProfile) ?? [emptyProfile()];
  selectedProfileId.value = set?.activeProfileId ?? profiles.value[0].id;
  loadDraft(profiles.value.find(profile => profile.id === selectedProfileId.value) ?? profiles.value[0]);
  showApiKey.value = false;
  showDialog.value = true;
}

function switchProfile(profileId: string) {
  commitDraft();
  selectedProfileId.value = profileId;
  const profile = profiles.value.find(item => item.id === profileId);
  if (profile) loadDraft(profile);
  showApiKey.value = false;
}

function addProfile() {
  commitDraft();
  const id = `profile-${Date.now()}-${profiles.value.length + 1}`;
  const profile = emptyProfile(id, `环境 ${profiles.value.length + 1}`);
  profiles.value.push(profile);
  selectedProfileId.value = id;
  loadDraft(profile);
  showApiKey.value = false;
}

function deleteProfile() {
  if (profiles.value.length <= 1 || !selectedProfileId.value) return;
  const index = profiles.value.findIndex(profile => profile.id === selectedProfileId.value);
  profiles.value = profiles.value.filter(profile => profile.id !== selectedProfileId.value);
  const next = profiles.value[Math.max(0, index - 1)] ?? profiles.value[0];
  selectedProfileId.value = next.id;
  loadDraft(next);
  showApiKey.value = false;
}

function resetDiscoveredModels() {
  models.value = [];
  draft.models = [];
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
    draft.models = result.map(model => ({...model}));
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
    // 先单独校验当前草稿，避免多个环境中某个输入错误时被静默丢弃。
    normalizeAiServiceProfile(draft, draft.id, draft.name);
    commitDraft();
    const normalizedSet = normalizeAiServiceConfigSet({
      version: 1,
      activeProfileId: selectedProfileId.value,
      profiles: profiles.value,
    });
    savedSet.value = await saveAiServiceConfigSet(normalizedSet);
    showDialog.value = false;
    $q.notify({type: 'positive', message: 'AI 服务环境已保存'});
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
    savedSet.value = null;
    profiles.value = [];
    selectedProfileId.value = null;
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

}

.model-select {
  min-width: 0;
}

.profile-select {
  min-width: 0;
}

:global(.settings-select-popup) {
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
}

:global(.settings-select-popup .q-item) {
  color: var(--pad-text-color-200);
}
</style>
