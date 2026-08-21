<template>
  <q-item clickable v-ripple class="settings-item" @click="openDialog">
    <q-item-section avatar class="settings-icon-section">
      <q-icon name="monitor_heart"/>
    </q-item-section>
    <q-item-section>
      <q-item-label class="label-text text-weight-medium">诊断信息</q-item-label>
      <q-item-label caption class="desc-text">查看运行日志与耗时统计</q-item-label>
    </q-item-section>
    <q-item-section side>
      <q-icon name="chevron_right" class="desc-text"/>
    </q-item-section>
  </q-item>

  <q-dialog v-model="showDialog" no-refocus>
    <q-card class="diagnostic-dialog">
      <q-card-section class="diagnostic-heading">
        <div>
          <div class="text-h6 title-text">诊断信息</div>
          <div class="text-caption desc-text">
            {{ fileName ? `${fileName} · 最新日志位于底部` : '当前应用的运行日志与耗时统计' }}
          </div>
        </div>
        <q-space/>
        <q-btn icon="close" flat round dense v-close-popup aria-label="关闭诊断信息弹窗"/>
      </q-card-section>
      <q-separator/>

      <div ref="contentElement" class="diagnostic-content">
        <div v-if="loading" class="diagnostic-state">
          <q-spinner color="primary" size="28px"/>
          <span>正在读取诊断信息…</span>
        </div>
        <div v-else-if="error" class="diagnostic-state diagnostic-error">
          <q-icon name="error_outline" size="28px"/>
          <span>读取诊断信息失败：{{ error }}</span>
        </div>
        <div v-else-if="!content" class="diagnostic-state">
          <q-icon name="description" size="28px"/>
          <span>当前还没有诊断信息</span>
        </div>
        <pre v-else class="diagnostic-log">{{ content }}</pre>
      </div>

      <q-separator/>
      <q-card-actions align="right">
        <q-btn flat icon="refresh" label="刷新" color="primary" :loading="loading" @click="refresh"/>
        <q-btn
          flat
          icon="content_copy"
          label="复制全部"
          color="primary"
          :disable="!content || loading"
          @click="copyAll"
        />
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {nextTick, ref} from 'vue';
import {useQuasar} from 'quasar';
import {copyTextToClipboard} from '../../utils/clipboard';
import {loadDiagnosticLog} from '../../utils/diagnosticLog';
import {formatError} from '../../utils/formatError';

const $q = useQuasar();
const showDialog = ref(false);
const loading = ref(false);
const error = ref<string | null>(null);
const fileName = ref('');
const content = ref('');
const contentElement = ref<HTMLElement | null>(null);

async function openDialog() {
  showDialog.value = true;
  await refresh();
}

async function refresh() {
  loading.value = true;
  error.value = null;
  try {
    const snapshot = await loadDiagnosticLog();
    fileName.value = snapshot?.fileName ?? '';
    content.value = snapshot?.content ?? '';
  } catch (loadError) {
    fileName.value = '';
    content.value = '';
    error.value = formatError(loadError);
  } finally {
    loading.value = false;
    await nextTick();
    contentElement.value?.scrollTo({top: contentElement.value.scrollHeight});
  }
}

async function copyAll() {
  if (!content.value) return;
  try {
    await copyTextToClipboard(content.value);
    $q.notify({type: 'positive', message: '诊断信息已复制'});
  } catch (copyError) {
    $q.notify({type: 'negative', message: `复制诊断信息失败：${formatError(copyError)}`});
  }
}
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>
<style scoped lang="scss">
.diagnostic-dialog {
  display: flex;
  flex-direction: column;
  width: min(960px, 94vw);
  height: min(760px, 88vh);
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-200);
  border-radius: var(--pad-radius-xl);
}

.diagnostic-heading {
  display: flex;
  align-items: center;
  gap: 12px;
}

.title-text {
  color: var(--pad-text-color-100);
}

.desc-text,
.diagnostic-state {
  color: var(--pad-text-color-400);
}

.diagnostic-content {
  flex: 1;
  min-height: 0;
  padding: 0;
  overflow: auto;
  background: var(--pad-bg-color-100);
}

.diagnostic-state {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  gap: 10px;
}

.diagnostic-error {
  padding: 20px;
  color: var(--pad-danger-color);
}

.diagnostic-log {
  width: 100%;
  margin: 0;
  padding: 16px;
  box-sizing: border-box;
  color: var(--pad-text-color-200);
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
  line-height: 1.55;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

@media (max-width: 600px) {
  .diagnostic-dialog {
    width: calc(100vw - 24px);
    height: min(82vh, 720px);
  }

  .diagnostic-log {
    padding: 12px;
    font-size: 11px;
  }
}
</style>
