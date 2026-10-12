<template>
  <q-item clickable v-ripple class="settings-item" @click="openDialog">
    <q-item-section avatar class="settings-icon-section">
      <q-icon name="monitor_heart"/>
    </q-item-section>
    <q-item-section>
      <q-item-label class="label-text text-weight-medium">诊断信息</q-item-label>
      <q-item-label caption class="desc-text">查看运行日志与改密耗时统计</q-item-label>
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
            {{ tab === 'keyChange' ? '最近一次改密的匿名性能报告，可复制后提供诊断' : fileName ? `${fileName} · 最新日志位于底部` : '当前应用的运行日志与耗时统计' }}
          </div>
        </div>
        <q-space/>
        <q-btn icon="close" flat round dense v-close-popup aria-label="关闭诊断信息弹窗"/>
      </q-card-section>
      <q-separator/>
      <q-tabs v-model="tab" dense align="left" active-color="primary" indicator-color="primary" class="diagnostic-tabs">
        <q-tab name="keyChange" label="改密统计"/>
        <q-tab name="logs" label="运行日志"/>
      </q-tabs>
      <q-separator/>

      <template v-if="tab === 'keyChange' && report && !loading && !reportError">
        <q-card-section class="report-summary">
          <div class="report-status">{{ keyChangeReportOutcome(report.outcome) }} · {{ report.completedFiles }} / {{ report.encryptedFiles }} 个对象</div>
          <div class="report-grid">
            <span>后端活跃用时</span><strong>{{ reportDuration(report.activeMs) }}</strong>
            <span>实际读取内容</span><strong>{{ formatBytes(report.readBytes) }}</strong>
            <span>供给写入端内容</span><strong>{{ formatBytes(report.writeStreamBytes) }}</strong>
            <span>同步加解密 / 哈希</span><strong>{{ reportDuration(report.cryptoMs) }} / {{ reportDuration(report.hashMs) }}</strong>
          </div>
          <div class="text-caption desc-text">读取与上传可能重叠，不应将各操作耗时相加；完整统计口径见下方 notes。</div>
          <div v-if="report.incompleteHistory" class="text-caption report-warning">曾中断或缺少早期统计，此报告只包含已保留的部分。</div>
        </q-card-section>
        <q-separator/>
        <JsonTreeViewer :source="reportSource"/>
      </template>
      <div v-else-if="tab === 'keyChange'" class="diagnostic-content">
        <div class="diagnostic-state">
          <template v-if="loading"><q-spinner color="primary" size="28px"/><span>正在读取改密统计…</span></template>
          <span v-else-if="reportError" class="diagnostic-error">读取改密统计失败：{{ reportError }}</span>
          <span v-else>暂无改密统计。完成一次改密后可在这里查看和复制。</span>
        </div>
      </div>

      <div v-else ref="contentElement" class="diagnostic-content">
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
          :label="tab === 'keyChange' ? '复制改密报告' : '复制全部'"
          color="primary"
          :disable="(tab === 'keyChange' ? !reportSource : !content) || loading"
          @click="copyAll"
        />
        <q-btn flat label="关闭" color="primary" v-close-popup/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, nextTick, ref} from 'vue';
import {useQuasar} from 'quasar';
import {copyTextToClipboard} from '../../utils/clipboard';
import {loadDiagnosticLog} from '../../utils/diagnosticLog';
import {formatError} from '../../utils/formatError';
import api from '../../utils/api';
import type {KeyChangeReport} from '../../bindings';
import {loadAboutInfo} from '../../utils/aboutInfo';
import {formatBytes} from '../../utils/format';
import {keyChangeReportSource, keyChangeReportOutcome, reportDuration} from '../../utils/vaultKeyChangeReport';
import JsonTreeViewer from '../../components/JsonTreeViewer.vue';

const $q = useQuasar();
const showDialog = ref(false);
const loading = ref(false);
const error = ref<string | null>(null);
const fileName = ref('');
const content = ref('');
const contentElement = ref<HTMLElement | null>(null);
const tab = ref('logs');
const report = ref<KeyChangeReport | null>(null);
const reportError = ref('');
const buildContext = ref({appVersion: '未知', gitCommit: '未知'});
const reportSource = computed(() => report.value ? keyChangeReportSource(report.value, buildContext.value) : undefined);

async function openDialog() {
  showDialog.value = true;
  await refresh();
  if (report.value) tab.value = 'keyChange';
}

async function refresh() {
  loading.value = true;
  error.value = null;
  reportError.value = '';
  try {
    await Promise.all([
      (async () => {
        try {
          const snapshot = await loadDiagnosticLog();
          fileName.value = snapshot?.fileName ?? '';
          content.value = snapshot?.content ?? '';
        } catch (loadError) {
          fileName.value = ''; content.value = ''; error.value = formatError(loadError);
        }
      })(),
      (async () => {
        try { report.value = await api.cmdGetVaultKeyChangeReport(); }
        catch (loadError) { report.value = null; reportError.value = formatError(loadError); }
        try { buildContext.value = await loadAboutInfo(); } catch { /* 构建信息失败不阻止查看实测报告。 */ }
      })(),
    ]);
  } finally {
    loading.value = false;
    await nextTick();
    contentElement.value?.scrollTo({top: contentElement.value.scrollHeight});
  }
}

async function copyAll() {
  const text = tab.value === 'keyChange' ? reportSource.value ? JSON.stringify(reportSource.value, null, 2) : '' : content.value;
  if (!text) return;
  try {
    await copyTextToClipboard(text);
    $q.notify({type: 'positive', message: tab.value === 'keyChange' ? '改密报告已复制' : '诊断信息已复制'});
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

.diagnostic-heading, .diagnostic-tabs, .report-summary { flex-shrink: 0; }
.diagnostic-tabs { color: var(--pad-text-color-200); background: var(--pad-bg-color-200); }
.report-summary { display: flex; flex-direction: column; gap: 8px; padding: 12px 16px; }
.report-grid { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 4px 14px; font-size: 13px; }
.report-grid span { color: var(--pad-text-color-400); }
.report-grid strong { overflow-wrap: anywhere; }
.report-status { color: var(--pad-text-color-100); }
.report-warning { color: var(--pad-danger-color); }

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
