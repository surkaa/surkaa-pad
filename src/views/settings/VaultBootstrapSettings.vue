<template>
  <q-item clickable v-ripple class="settings-item" @click="openDialog">
    <q-item-section avatar class="settings-icon-section">
      <q-icon name="key"/>
    </q-item-section>
    <q-item-section>
      <q-item-label class="label-text text-weight-medium">密钥派生配置</q-item-label>
      <q-item-label caption class="desc-text">{{ entrySummary }}</q-item-label>
    </q-item-section>
    <q-item-section side>
      <q-icon name="chevron_right" class="desc-text"/>
    </q-item-section>
  </q-item>

  <q-dialog v-model="showDialog" no-refocus>
    <q-card class="bootstrap-dialog">
      <q-card-section class="row items-start no-wrap q-pb-sm">
        <div>
          <div class="text-h6 title-text">密钥派生配置</div>
          <div class="text-caption desc-text">
            这些参数用于从主密码派生数据密钥，不包含主密码或密钥本身。
          </div>
        </div>
        <q-space/>
        <q-btn icon="close" flat round dense v-close-popup aria-label="关闭密钥派生配置弹窗"/>
      </q-card-section>
      <q-separator/>

      <q-card-section v-if="loading" class="bootstrap-state">
        <q-spinner color="primary" size="28px"/>
        <span>正在读取配置…</span>
      </q-card-section>
      <q-card-section v-else-if="loadError" class="bootstrap-state error-text">
        <q-icon name="error_outline" size="28px"/>
        <span>{{ loadError }}</span>
      </q-card-section>
      <q-card-section v-else-if="bootstrap" class="bootstrap-content">
        <div class="detail-grid">
          <span>算法</span><strong>Argon2id v{{ bootstrap.kdf.algorithmVersion }}</strong>
          <span>盐</span><code>{{ bootstrap.kdf.salt }}</code>
          <span>内存成本</span><strong>{{ formatKiB(bootstrap.kdf.memoryCostKib) }}</strong>
          <span>时间成本</span><strong>{{ bootstrap.kdf.timeCost }}</strong>
          <span>并行度</span><strong>{{ bootstrap.kdf.parallelism }}</strong>
          <span>输出长度</span><strong>{{ bootstrap.kdf.outputLength }} 字节</strong>
          <span>Vault ID</span><code>{{ bootstrap.vaultId }}</code>
        </div>
        <div class="bootstrap-notice">
          <q-icon name="info_outline"/>
          <span>修改任意参数都会产生不同密钥，因此当前版本只允许导入与现有 Vault 完全匹配的配置。</span>
        </div>
      </q-card-section>

      <q-separator/>
      <q-card-actions align="right" class="q-px-md q-pb-md">
        <q-btn flat label="导入" color="primary" :disable="loading" @click="openImportDialog"/>
        <q-btn flat label="查看 JSON" color="primary" :disable="!bootstrap || loading" @click="showJson = true"/>
        <q-btn
          unelevated
          icon="content_copy"
          label="复制配置"
          color="primary"
          :disable="!bootstrap || loading"
          @click="copyConfig"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>

  <JsonSourceDialog
    v-model="showJson"
    title="密钥派生配置 JSON"
    :source="bootstrap"
    copy-label="复制配置"
    copy-success-message="密钥派生配置已复制"
  />

  <q-dialog v-model="showImport" persistent no-refocus>
    <q-card class="import-dialog">
      <q-card-section>
        <div class="text-h6 title-text">导入密钥派生配置</div>
        <div class="text-caption desc-text">
          粘贴完整 JSON 并输入当前主密码。验证全部通过前不会修改本地或云端配置。
        </div>
      </q-card-section>
      <q-card-section class="q-pt-none q-gutter-y-md">
        <q-input
          v-model="importJson"
          type="textarea"
          outlined
          autogrow
          label="配置 JSON"
          :disable="importing"
          class="themed-input"
        />
        <q-input
          v-model="importPassword"
          type="password"
          outlined
          label="当前主密码"
          :disable="importing"
          class="themed-input"
          @keyup.enter="importConfig"
        />
      </q-card-section>
      <q-card-actions align="right" class="q-px-md q-pb-md">
        <q-btn flat label="取消" class="secondary-action" :disable="importing" v-close-popup/>
        <q-btn
          unelevated
          label="验证并导入"
          color="primary"
          :loading="importing"
          :disable="!importJson.trim() || !importPassword"
          @click="importConfig"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, onMounted, ref} from 'vue';
import {useQuasar} from 'quasar';
import type {VaultBootstrap} from '../../bindings';
import JsonSourceDialog from '../../components/JsonSourceDialog.vue';
import {copyTextToClipboard} from '../../utils/clipboard';
import {formatError} from '../../utils/formatError';
import {formatKiB} from '../../utils';
import api from '../../utils/api';

const props = defineProps<{remoteEnabled: boolean}>();
const $q = useQuasar();
const bootstrap = ref<VaultBootstrap>();
const loading = ref(false);
const loadError = ref('');
const showDialog = ref(false);
const showJson = ref(false);
const showImport = ref(false);
const importJson = ref('');
const importPassword = ref('');
const importing = ref(false);

const entrySummary = computed(() => {
  if (!bootstrap.value) return loadError.value || '查看、复制或导入当前 Vault 的派生参数';
  const location = props.remoteEnabled ? '已同步到云端' : '仅保存在本机';
  return `Argon2id · ${formatKiB(bootstrap.value.kdf.memoryCostKib)} · ${location}`;
});

async function refresh() {
  loading.value = true;
  loadError.value = '';
  try {
    bootstrap.value = await api.cmdGetVaultBootstrap();
  } catch (error) {
    bootstrap.value = undefined;
    loadError.value = `读取配置失败：${formatError(error)}`;
  } finally {
    loading.value = false;
  }
}

async function openDialog() {
  showDialog.value = true;
  await refresh();
}

async function copyConfig() {
  try {
    await copyTextToClipboard(await api.cmdExportVaultBootstrap());
    $q.notify({type: 'positive', message: '密钥派生配置已复制'});
  } catch (error) {
    $q.notify({type: 'negative', message: `复制配置失败：${formatError(error)}`});
  }
}

function openImportDialog() {
  importJson.value = '';
  importPassword.value = '';
  showImport.value = true;
}

async function importConfig() {
  if (!importJson.value.trim() || !importPassword.value || importing.value) return;
  importing.value = true;
  try {
    bootstrap.value = await api.cmdImportVaultBootstrap(
      importJson.value.trim(),
      importPassword.value,
    );
    showImport.value = false;
    importJson.value = '';
    importPassword.value = '';
    $q.notify({type: 'positive', message: '密钥派生配置验证并导入成功'});
  } catch (error) {
    $q.notify({type: 'negative', message: `导入配置失败：${formatError(error)}`});
  } finally {
    importing.value = false;
  }
}

onMounted(refresh);
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>
<style scoped lang="scss">
.bootstrap-dialog,
.import-dialog {
  width: min(620px, calc(100vw - 24px));
  max-width: 620px;
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
}

.title-text,
.detail-grid strong,
.detail-grid code {
  color: var(--pad-text-color-200);
}

.desc-text,
.bootstrap-state,
.detail-grid span {
  color: var(--pad-text-color-400);
}

.bootstrap-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 150px;
  gap: 10px;
}

.error-text {
  color: var(--pad-danger-color);
}

.bootstrap-content {
  display: grid;
  gap: 16px;
}

.detail-grid {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 10px 18px;
  align-items: center;
}

.detail-grid strong,
.detail-grid code {
  min-width: 0;
  overflow-wrap: anywhere;
}

.bootstrap-notice {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  color: var(--pad-text-color-300);
  background: var(--pad-bg-color-100);
  border: 1px solid var(--pad-border-color-100);
  border-radius: 10px;
  font-size: 0.82rem;
}

.themed-input :deep(.q-field__native),
.themed-input :deep(.q-field__input),
.themed-input :deep(.q-field__label) {
  color: var(--pad-text-color-200);
}

.themed-input :deep(.q-field__control::before) {
  border-color: var(--pad-border-color-100);
}

@media (max-width: 600px) {
  .detail-grid {
    grid-template-columns: 1fr;
    gap: 3px;
  }

  .detail-grid strong,
  .detail-grid code {
    margin-bottom: 7px;
  }
}
</style>
