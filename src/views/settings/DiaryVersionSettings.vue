<template>
  <q-item clickable v-ripple class="settings-item" @click="showDialog = true">
    <q-item-section avatar class="settings-icon-section">
      <q-icon name="manage_search"/>
    </q-item-section>
    <q-item-section>
      <q-item-label class="label-text text-weight-medium">日记数据版本</q-item-label>
      <q-item-label caption class="desc-text">{{ entrySummary }}</q-item-label>
    </q-item-section>
    <q-item-section side>
      <q-icon name="chevron_right" class="desc-text"/>
    </q-item-section>
  </q-item>

  <q-dialog v-model="showDialog" :persistent="running">
    <q-card class="version-dialog">
      <q-card-section>
        <div class="text-h6 dialog-title">日记数据版本</div>
        <div class="text-caption dialog-description">
          当前应用使用{{ currentVersionLabel }}。检查会读取并解密当前存储中的日记主文件，
          不修改日记，也不下载附件；云同步开启时会产生少量网络请求。
        </div>
      </q-card-section>

      <q-card-section v-if="running" class="q-pt-none">
        <div class="operation-heading">{{ operationTitle }}</div>
        <div class="dialog-description q-mt-xs">{{ progressText }}</div>
        <div v-if="display.currentDiaryId" class="current-diary ellipsis q-mt-sm">
          {{ display.currentDiaryId }}
          <span v-if="outcomeText">· {{ outcomeText }}</span>
        </div>
        <q-linear-progress
          v-if="display.total > 0"
          rounded
          size="8px"
          color="primary"
          :value="Math.min(display.processed / display.total, 1)"
          class="q-mt-md"
        />
        <div v-else class="q-mt-md"><q-spinner color="primary" size="24px"/></div>
      </q-card-section>

      <q-card-section v-else-if="display.phase === 'failed'" class="q-pt-none">
        <div class="notice notice-negative">
          <q-icon name="error_outline"/>
          <span>{{ display.error }}</span>
        </div>
      </q-card-section>

      <q-card-section v-else-if="display.phase === 'cancelled' && !auditReport" class="q-pt-none">
        <div class="notice notice-warning">
          <q-icon name="cancel"/>
          <span>操作已取消，已处理 {{ display.processed }} / {{ display.total }} 篇日记；本次结果不完整。</span>
        </div>
      </q-card-section>

      <q-card-section v-else-if="auditReport" class="q-pt-none report-section">
        <div v-if="display.phase === 'cancelled'" class="notice notice-warning">
          <q-icon name="cancel"/>
          <span>刚才的操作已取消，下面仍显示上一次完整检查的结果。</span>
        </div>
        <div class="report-meta">
          <span>{{ scopeText(auditReport.scope) }}</span>
          <span>{{ auditReport.totalDiaries }} 篇日记</span>
          <span>当前 V{{ auditReport.currentVersion }}</span>
        </div>

        <div class="version-breakdown">{{ formatDiaryVersionBreakdown(auditReport) }}</div>

        <div class="count-grid">
          <div class="count-card">
            <strong>{{ auditReport.currentDiaries }}</strong>
            <span>当前版本</span>
          </div>
          <div class="count-card" :class="{'count-warning': auditReport.legacyDiaries > 0}">
            <strong>{{ auditReport.legacyDiaries }}</strong>
            <span>旧版</span>
          </div>
          <div class="count-card" :class="{'count-warning': auditReport.newerDiaries > 0}">
            <strong>{{ auditReport.newerDiaries }}</strong>
            <span>更高版本</span>
          </div>
          <div class="count-card" :class="{'count-negative': auditReport.failedDiaries > 0}">
            <strong>{{ auditReport.failedDiaries }}</strong>
            <span>检查失败</span>
          </div>
        </div>

        <div v-if="allCurrent" class="notice notice-positive">
          <q-icon name="check_circle"/>
          <span>全部日记均已使用当前数据格式。</span>
        </div>
        <div v-else-if="auditReport.legacyDiaries > 0" class="notice notice-warning">
          <q-icon name="upgrade"/>
          <span>发现 {{ auditReport.legacyDiaries }} 篇旧版日记，可显式批量升级。</span>
        </div>
        <div v-if="auditReport.newerDiaries > 0" class="notice notice-warning">
          <q-icon name="new_releases"/>
          <span>{{ auditReport.newerDiaries }} 篇日记来自更高版本应用，本应用不会修改它们。</span>
        </div>
        <div v-if="auditReport.failedDiaries > 0" class="notice notice-negative">
          <q-icon name="error_outline"/>
          <span>{{ auditReport.failedDiaries }} 篇日记无法确认版本，请先排查后再移除旧版兼容代码。</span>
        </div>
        <div v-if="lastUpgradeReport" class="notice notice-neutral">
          <q-icon name="published_with_changes"/>
          <span>
            上次升级成功 {{ lastUpgradeReport.upgradedDiaries }} 篇，失败 {{ lastUpgradeReport.failedDiaries }} 篇；以上为升级后的复查结果。
          </span>
        </div>

        <q-expansion-item
          v-if="auditReport.failedDiaryIds.length > 0"
          dense
          icon="list_alt"
          :label="`失败日记 ID（显示 ${auditReport.failedDiaryIds.length} 个，最多 20 个）`"
          class="failed-list"
        >
          <div v-for="diaryId in auditReport.failedDiaryIds" :key="diaryId" class="failed-id">
            {{ diaryId }}
          </div>
        </q-expansion-item>
      </q-card-section>

      <q-card-section v-else class="q-pt-none">
        <div class="notice notice-neutral">
          <q-icon name="info_outline"/>
          <span>检查完成前不会自动迁移任何日记。检查结果仅保留在当前运行期间。</span>
        </div>
      </q-card-section>

      <q-card-actions align="right" class="q-px-md q-pb-md">
        <template v-if="running">
          <q-btn
            flat
            label="取消"
            class="secondary-action"
            :loading="cancelling"
            :disable="!activeTaskToken"
            @click="cancelOperation"
          />
        </template>
        <template v-else>
          <q-btn flat label="关闭" class="secondary-action" v-close-popup/>
          <q-btn
            v-if="auditReport?.legacyDiaries"
            outline
            label="升级旧版日记"
            color="primary"
            @click="showUpgradeConfirm = true"
          />
          <q-btn
            unelevated
            :label="auditReport ? '重新检查' : '开始检查'"
            color="primary"
            @click="runOperation('inspect')"
          />
        </template>
      </q-card-actions>
    </q-card>
  </q-dialog>

  <q-dialog v-model="showUpgradeConfirm" persistent>
    <q-card class="confirm-dialog">
      <q-card-section>
        <div class="text-h6 dialog-title">升级旧版日记</div>
        <div class="dialog-description q-mt-sm">
          将逐篇迁移 {{ auditReport?.legacyDiaries ?? 0 }} 篇旧版日记到
          V{{ auditReport?.currentVersion }}。
          单篇失败不会中断其他日记；升级期间请勿关闭应用。
        </div>
      </q-card-section>
      <q-card-actions align="right" class="q-px-md q-pb-md">
        <q-btn flat label="取消" class="secondary-action" v-close-popup/>
        <q-btn unelevated label="开始升级" color="primary" @click="confirmUpgrade"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {Channel} from '@tauri-apps/api/core';
import {useQuasar} from 'quasar';
import {computed, onBeforeUnmount, ref, watch} from 'vue';
import type {
  DiaryVersionEvent,
  DiaryVersionOperation,
  DiaryVersionReport,
  DiaryVersionStorageScope,
} from '../../bindings';
import api from '../../utils/api';
import {
  diaryVersionOutcomeText,
  formatDiaryVersionBreakdown,
  initialDiaryVersionDisplay,
  isDiaryVersionReportCurrent,
  reduceDiaryVersionEvent,
  withDiaryVersionError,
} from '../../utils/diaryVersion';
import {formatError} from '../../utils/formatError';

const $q = useQuasar();
const props = defineProps<{
  remoteEnabled: boolean;
}>();
const showDialog = ref(false);
const showUpgradeConfirm = ref(false);
const display = ref(initialDiaryVersionDisplay());
const auditReport = ref<DiaryVersionReport>();
const lastUpgradeReport = ref<DiaryVersionReport>();
const activeTaskToken = ref('');
const cancelling = ref(false);
let operationRevision = 0;

const running = computed(() => display.value.phase === 'running');
const allCurrent = computed(() => auditReport.value
  ? isDiaryVersionReportCurrent(auditReport.value)
  : false);
const currentVersionLabel = computed(() => auditReport.value
  ? ` V${auditReport.value.currentVersion}`
  : '当前数据格式');
const operationTitle = computed(() => display.value.operation === 'upgrade'
  ? '正在升级旧版日记…'
  : lastUpgradeReport.value
    ? '正在复查升级结果…'
    : '正在检查日记版本…');
const progressText = computed(() => display.value.total > 0
  ? `${display.value.processed} / ${display.value.total}`
  : '正在读取日记清单…');
const outcomeText = computed(() => diaryVersionOutcomeText(display.value.currentOutcome));
const entrySummary = computed(() => {
  const report = auditReport.value;
  if (!report) return '检查全部日记是否已升级为当前格式';
  if (isDiaryVersionReportCurrent(report)) return `已检查：${report.totalDiaries} 篇均为 V${report.currentVersion}`;
  const issues = [];
  if (report.legacyDiaries) issues.push(`${report.legacyDiaries} 篇旧版`);
  if (report.newerDiaries) issues.push(`${report.newerDiaries} 篇更高版本`);
  if (report.failedDiaries) issues.push(`${report.failedDiaries} 篇失败`);
  return `已检查：${issues.join(' · ') || '结果不完整'}`;
});

function scopeText(scope: DiaryVersionStorageScope): string {
  return scope === 'cloud' ? '云端存储' : '本地存储';
}

async function runOperation(operation: DiaryVersionOperation) {
  if (running.value) return;
  const revision = ++operationRevision;
  const terminal = {received: false};
  display.value = initialDiaryVersionDisplay(operation);
  activeTaskToken.value = '';
  cancelling.value = false;

  const event = new Channel<DiaryVersionEvent>();
  event.onmessage = (message) => {
    if (revision !== operationRevision) return;
    display.value = reduceDiaryVersionEvent(display.value, message);

    if (message.event === 'completed') {
      terminal.received = true;
      activeTaskToken.value = '';
      cancelling.value = false;
      if (operation === 'inspect') {
        auditReport.value = message.data.report;
      } else {
        lastUpgradeReport.value = message.data.report;
        void runOperation('inspect');
      }
    } else if (message.event === 'cancelled' || message.event === 'error') {
      terminal.received = true;
      activeTaskToken.value = '';
      cancelling.value = false;
    }
  };

  try {
    const token = operation === 'inspect'
      ? await api.cmdInspectDiaryVersions(event)
      : await api.cmdUpgradeLegacyDiaries(event);
    if (revision === operationRevision && !terminal.received) {
      activeTaskToken.value = token;
    }
  } catch (error) {
    if (revision !== operationRevision || terminal.received) return;
    display.value = withDiaryVersionError(display.value, formatError(error));
    activeTaskToken.value = '';
    cancelling.value = false;
  }
}

async function cancelOperation() {
  if (!activeTaskToken.value || cancelling.value) return;
  cancelling.value = true;
  try {
    const accepted = await api.cmdCancelTask(activeTaskToken.value);
    if (!accepted) {
      cancelling.value = false;
      $q.notify({type: 'warning', message: '任务已经结束或无法取消'});
    }
  } catch (error) {
    cancelling.value = false;
    $q.notify({type: 'negative', message: `取消任务失败：${formatError(error)}`});
  }
}

function confirmUpgrade() {
  showUpgradeConfirm.value = false;
  lastUpgradeReport.value = undefined;
  void runOperation('upgrade');
}

watch(() => props.remoteEnabled, () => {
  auditReport.value = undefined;
  lastUpgradeReport.value = undefined;
  if (!running.value) {
    display.value = initialDiaryVersionDisplay();
  }
});

onBeforeUnmount(() => {
  operationRevision += 1;
  if (activeTaskToken.value) {
    void api.cmdCancelTask(activeTaskToken.value).catch(() => undefined);
  }
});
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>

<style scoped lang="scss">
.version-dialog,
.confirm-dialog {
  width: min(620px, calc(100vw - 24px));
  max-width: 620px;
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-100);
  border-radius: var(--pad-radius-xl);
}

.dialog-title,
.operation-heading,
.count-card strong {
  color: var(--pad-text-color-200);
}

.dialog-description,
.current-diary,
.report-meta,
.count-card span {
  color: var(--pad-text-color-400);
}

.operation-heading {
  font-weight: 600;
}

.current-diary {
  font-size: 0.78rem;
}

.report-section {
  display: grid;
  gap: 12px;
}

.report-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 14px;
  font-size: 0.78rem;
}

.version-breakdown {
  color: var(--pad-text-color-300);
  font-size: 0.9rem;
}

.count-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}

.count-card {
  display: grid;
  gap: 2px;
  padding: 10px;
  text-align: center;
  background: var(--pad-bg-color-200);
  border: 1px solid var(--pad-border-color-100);
  border-radius: 10px;

  strong {
    font-size: 1.15rem;
  }

  span {
    font-size: 0.72rem;
  }
}

.count-warning strong {
  color: var(--q-warning);
}

.count-negative strong {
  color: var(--q-negative);
}

.notice {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  color: var(--pad-text-color-300);
  background: var(--pad-bg-color-200);
  border: 1px solid var(--pad-border-color-100);
  border-radius: 10px;
  font-size: 0.84rem;

  .q-icon {
    flex: 0 0 auto;
    margin-top: 1px;
    font-size: 18px;
  }
}

.notice-positive .q-icon {
  color: var(--q-positive);
}

.notice-warning .q-icon {
  color: var(--q-warning);
}

.notice-negative .q-icon {
  color: var(--q-negative);
}

.notice-neutral .q-icon {
  color: var(--pad-primary-dark);
}

.failed-list {
  color: var(--pad-text-color-300);
  background: var(--pad-bg-color-200);
  border-radius: 10px;

  :deep(.q-item) {
    color: var(--pad-text-color-300);
  }
}

.failed-id {
  padding: 5px 16px;
  color: var(--pad-text-color-400);
  font-family: monospace;
  overflow-wrap: anywhere;
}

.secondary-action {
  color: var(--pad-text-color-300);
}

@media (max-width: 520px) {
  .count-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
