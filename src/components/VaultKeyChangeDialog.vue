<template>
  <q-dialog :model-value="modelValue" persistent no-refocus @update:model-value="value => emit('update:modelValue', value)">
    <q-card class="key-change-dialog">
      <q-card-section class="key-change-header">
        <div class="text-h6">{{ resumeMode ? '继续改密' : '更改密码与盐' }}</div>
        <div class="text-caption desc-text">迁移期间暂停日记、附件及 AI 操作；中断后可续改，开始替换后不能取消或回滚。</div>
      </q-card-section>
      <q-separator />
      <q-card-section class="key-change-content">
        <template v-if="!plan && !busy">
          <template v-if="resumeMode">
            <div class="notice">发现未完成的改密。请输入新主密码继续处理；只更换盐时仍输入原密码。请勿清理应用数据或改密记录。</div>
            <q-input v-model="resumePassword" label="续改主密码（只换盐时输入原密码）" type="password" outlined autofocus @keyup.enter="resumeChange" />
          </template>
          <template v-else>
            <q-input v-model="oldPassword" label="当前主密码" type="password" outlined autofocus />
            <q-toggle v-model="changePassword" label="同时更改密码（关闭时只更换随机盐）" color="primary" />
            <template v-if="changePassword">
              <q-input v-model="newPassword" label="新主密码" type="password" outlined />
              <q-input v-model="confirmPassword" label="再次输入新主密码" type="password" outlined />
            </template>
            <q-select v-model="memoryCost" :options="memoryOptions" emit-value map-options outlined label="派生内存成本" :dark="$q.dark.isActive" :options-dark="$q.dark.isActive" popup-content-class="key-change-memory-popup" />
            <div class="notice">每次都会生成新的随机盐。只换盐时密码保持不变，也需要重加密全部已加密数据；未加密附件保持原样。</div>
          </template>
          <q-checkbox v-model="devicesClosed" label="我确保已退出其他设备上的此应用，且不会在改密期间打开它们" color="negative" class="device-acknowledgement" />
        </template>

        <template v-if="plan && !busy">
          <div class="plan-grid">
            <span>完整数据量</span><strong>{{ plan.totalFiles }} 个对象 · {{ formatBytes(plan.totalBytes) }}</strong>
            <span>需要重加密</span><strong>{{ plan.encryptedFiles }} 个对象 · {{ formatBytes(plan.encryptedBytes) }}</strong>
            <span>派生实测</span><strong>{{ estimateDuration(plan.derivationMs) }}</strong>
            <span>加解密估算</span><strong>{{ estimateDuration(plan.cryptoEstimateMs) }}</strong>
            <span>{{ plan.remote ? '临时云端空间上限' : '额外本地空间上限' }}</span><strong>{{ formatBytes(plan.temporaryBytes) }}</strong>
            <template v-if="plan.remote"><span>累计传输量估算</span><strong>{{ formatBytes(plan.transferBytes) }}</strong></template>
          </div>
          <div v-if="plan.remote" class="text-caption desc-text">按需要重加密的数据量的 7 倍估算下载、上传和读回校验流量；备份与替换由云端复制，不经过本机传输。这不是最终占用空间或严格上限，不含改密检查点、请求开销及中断重试。</div>
          <div class="notice">耗时基于本机实测，不包含磁盘读写、校验及网络等待；云端改密还会产生读取、写入请求和流量。实际耗时可能明显更长。请备份重要数据并保持供电。{{ changePassword ? '中断续改时需要新主密码，请务必记住。' : '本次只更换盐，续改时仍输入原主密码。' }}</div>
          <div v-if="!plan.remote" class="notice">本次只更改本地数据，不会修改已断开云同步的旧云端副本或其他备份；新旧密钥的副本不能直接混用。</div>
          <div class="text-caption desc-text">新盐：{{ plan.newBootstrap.kdf.salt }}</div>
        </template>

        <div v-if="busy" class="progress-section">
          <div class="row items-center q-gutter-sm"><q-spinner color="primary" size="24px" /><span>{{ preparing ? '正在统计并进行派生、加密测试…' : keyChangeProgressLabel(progress?.phase ?? '') }}</span></div>
          <div v-if="!preparing" class="text-caption desc-text" data-testid="key-change-time-estimate">
            <div>{{ timeEstimateLabel }}</div>
            <div v-if="!pauseRequested">按本次运行的数据量与各步骤进度动态估算；网络、磁盘和请求等待会影响结果，仅供参考。</div>
          </div>
          <template v-if="progress">
            <div class="text-caption desc-text">已完成并保存检查点：{{ progress.completedFiles }} / {{ progress.totalFiles }} 个对象 · {{ formatBytes(progress.completedBytes) }} / {{ formatBytes(progress.totalBytes) }}</div>
            <div class="text-caption desc-text current-file">{{ progress.currentFile }}</div>
            <q-linear-progress :value="keyChangeProgressValue(progress)" color="primary" rounded size="6px" />
            <div v-if="progress.fileProgress" :key="progress.fileProgress.step" class="file-progress" data-testid="key-change-file-progress">
              <div class="row items-center justify-between q-gutter-xs">
                <span>{{ keyChangeFileProgressLabel(progress.fileProgress.step) }}</span>
                <strong>{{ Math.round(keyChangeFileProgressValue(progress.fileProgress) * 100) }}%</strong>
              </div>
              <q-linear-progress :value="keyChangeFileProgressValue(progress.fileProgress)" color="primary" rounded size="6px" />
              <div class="text-caption desc-text">{{ formatBytes(progress.fileProgress.completedBytes) }} / {{ formatBytes(progress.fileProgress.totalBytes) }}<template v-if="progress.fileProgress.completedBytes >= progress.fileProgress.totalBytes && progress.fileProgress.completedPoints < progress.fileProgress.totalPoints"> · 等待本步骤确认</template></div>
              <div class="text-caption desc-text">各步骤分别计数；云端服务端复制只显示开始与完成。</div>
            </div>
          </template>
          <div class="notice">请勿清理应用数据或改密记录。突然退出、断网或重启后，可以在解锁页面继续改密。</div>
        </div>
        <div v-if="failure" role="alert" class="failure">{{ failure }}</div>
      </q-card-section>
      <q-separator />
      <q-card-actions align="right" class="key-change-actions">
        <q-btn v-if="busy && !preparing" flat label="当前对象完成后暂停" color="primary" :disable="pauseRequested" @click="pauseChange" />
        <q-btn v-if="!busy" flat :label="resumeMode ? '稍后继续' : '取消'" color="primary" @click="close" />
        <q-btn v-if="!busy && resumeMode" unelevated label="继续改密" color="primary" :disable="!resumePassword || !devicesClosed" @click="resumeChange" />
        <q-btn v-else-if="!busy && plan" unelevated label="确认开始改密" color="primary" @click="startChange" />
        <q-btn v-else-if="!busy" unelevated label="统计数据并估算" color="primary" :disable="!canPrepare" @click="prepareChange" />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<script setup lang="ts">
import {computed, ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import {Channel} from '@tauri-apps/api/core';
import {useIntervalFn} from '@vueuse/core';
import api from '../utils/api';
import type {KeyChangePlan, KeyChangeProgress, LocalEncryptedConfig} from '../bindings';
import {formatBytes} from '../utils/format';
import {formatError} from '../utils/formatError';
import {estimateDuration, finishVaultKeyChange, getLocalEncryptedConfig, keyChangeCompletionMessage, keyChangeFileProgressLabel, keyChangeFileProgressValue, keyChangeProgressLabel, keyChangeProgressValue} from '../utils/vaultKeyChange';
import {useTimeoutStore} from '../stores/timeout';
import {KeyChangeTimeEstimator, keyChangeEstimatedCompletion} from '../utils/keyChangeTimeEstimate';

const props = defineProps<{modelValue: boolean; resume?: boolean; initialPassword?: string}>();
const emit = defineEmits<{(e: 'update:modelValue', value: boolean): void; (e: 'completed'): void}>();
const $q = useQuasar();
const timeout = useTimeoutStore();
const oldPassword = ref(''), newPassword = ref(''), confirmPassword = ref(''), resumePassword = ref('');
const changePassword = ref(true), devicesClosed = ref(false), resumeMode = ref(false);
const memoryCost = ref<number | null>(null);
const memoryOptions = [
  {label: '保持当前内存成本', value: null},
  ...(import.meta.env.DEV ? [{label: '1 MiB · 开发测试', value: 1024}] : []),
  {label: '64 MiB', value: 65536}, {label: '128 MiB', value: 131072}, {label: '256 MiB', value: 262144},
];
const plan = ref<KeyChangePlan | null>(null), progress = ref<KeyChangeProgress | null>(null);
const busy = ref(false), preparing = ref(false), pauseRequested = ref(false), failure = ref('');
const canPrepare = computed(() => !!oldPassword.value && devicesClosed.value && (!changePassword.value || (!!newPassword.value && newPassword.value === confirmPassword.value)));
const timeEstimator = new KeyChangeTimeEstimator();
const remainingMs = ref<number | null>(null), estimatedCompletion = ref<string | null>(null);
function refreshTimeEstimate() {
  remainingMs.value = timeEstimator.remainingMs(performance.now());
  estimatedCompletion.value = remainingMs.value === null ? null : keyChangeEstimatedCompletion(remainingMs.value, new Date());
}
const {pause: pauseEstimateTicker, resume: resumeEstimateTicker} = useIntervalFn(refreshTimeEstimate, 1000, {immediate: false});
const timeEstimateLabel = computed(() => {
  if (pauseRequested.value) return '将在当前对象完成后暂停';
  if (progress.value && (['switchingKey', 'savingLocalConfig'].includes(progress.value.phase)
    || (progress.value.totalBytes > 0 && progress.value.completedBytes >= progress.value.totalBytes))) {
    return '数据处理完成，正在完成最后确认';
  }
  if (remainingMs.value === null) return '预计完成时间：正在按数据量估算…';
  if (remainingMs.value === 0) return '即将完成，正在校验和保存检查点…';
  return `预计还需${estimateDuration(remainingMs.value)}${estimatedCompletion.value ? ` · 预计完成 ${estimatedCompletion.value}` : ''}`;
});

watch(() => props.modelValue, async (visible) => {
  if (!visible) { oldPassword.value = newPassword.value = confirmPassword.value = resumePassword.value = ''; return; }
  failure.value = ''; plan.value = null; progress.value = null; devicesClosed.value = false;
  resumeMode.value = props.resume ?? false;
  resumePassword.value = props.initialPassword ?? '';
  try { resumeMode.value ||= (await api.cmdGetVaultKeyChangeStatus()).pending; }
  catch (e) { failure.value = formatError(e); }
}, {immediate: true});

async function prepareChange() {
  if (!canPrepare.value || busy.value) return;
  busy.value = preparing.value = true; failure.value = '';
  timeout.clearCloseTimers();
  try {
    plan.value = await api.cmdPlanVaultKeyChange(oldPassword.value,
      changePassword.value ? newPassword.value : oldPassword.value,
      changePassword.value ? confirmPassword.value : oldPassword.value,
      memoryCost.value, await getLocalEncryptedConfig());
    oldPassword.value = newPassword.value = confirmPassword.value = '';
  } catch (e) { failure.value = formatError(e); }
  finally { busy.value = preparing.value = false; timeout.setTimeoutForCloseApp(); }
}

async function perform(run: (channel: Channel<KeyChangeProgress>) => Promise<LocalEncryptedConfig>) {
  if (busy.value) return;
  busy.value = true; pauseRequested.value = false; failure.value = '';
  progress.value = null;
  timeEstimator.reset(performance.now());
  refreshTimeEstimate();
  resumeEstimateTicker();
  timeout.clearCloseTimers();
  const event = new Channel<KeyChangeProgress>();
  let acceptingProgress = true;
  event.onmessage = value => {
    if (!acceptingProgress) return; // 旧运行的延迟事件不能覆盖续改后的测速样本。
    progress.value = value; timeEstimator.update(value); refreshTimeEstimate();
  };
  try {
    const local = await run(event);
    const biometricWasEnabled = await finishVaultKeyChange(local);
    $q.notify({type: 'positive', message: `${keyChangeCompletionMessage(biometricWasEnabled).replace(/。$/, '')}。可在设置 → 诊断信息 → 改密统计复制性能报告。`});
    emit('update:modelValue', false); emit('completed');
  } catch (e) {
    failure.value = formatError(e);
    try { resumeMode.value = (await api.cmdGetVaultKeyChangeStatus()).pending; } catch { resumeMode.value = true; }
    plan.value = null;
  } finally {
    acceptingProgress = false;
    pauseEstimateTicker();
    busy.value = false; resumePassword.value = '';
    timeout.setTimeoutForCloseApp();
  }
}

function startChange() { if (plan.value) return perform(channel => api.cmdStartVaultKeyChange(plan.value!.planId, channel)); }
function resumeChange() { if (resumePassword.value && devicesClosed.value) return perform(channel => api.cmdResumeVaultKeyChange(resumePassword.value, channel)); }
async function pauseChange() {
  pauseRequested.value = true;
  try { await api.cmdPauseVaultKeyChange(); }
  catch (e) { pauseRequested.value = false; failure.value = formatError(e); }
}
async function close() {
  if (busy.value) return;
  try {
    if (!resumeMode.value) await api.cmdDiscardVaultKeyChangePlan();
    emit('update:modelValue', false);
  } catch (e) { failure.value = formatError(e); }
}
</script>

<style scoped>
.key-change-dialog { width: 600px; max-width: calc(100vw - 24px); max-height: calc(100dvh - 32px); display: flex; flex-direction: column; overflow: hidden; background: var(--pad-bg-color-200); color: var(--pad-text-color-100); border-radius: var(--pad-radius-lg); }
.key-change-header, .key-change-actions { flex-shrink: 0; }
.key-change-content { min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
.desc-text { color: var(--pad-text-color-200); }
.notice { padding: 12px; border-radius: var(--pad-radius-md); background: var(--pad-bg-color-100); color: var(--pad-text-color-200); line-height: 1.65; font-size: 13px; }
.plan-grid { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 12px 20px; font-size: 14px; }
.plan-grid span { color: var(--pad-text-color-200); }
.plan-grid strong, .current-file { overflow-wrap: anywhere; }
.progress-section { display: flex; flex-direction: column; gap: 14px; }
.file-progress { padding: 12px; display: flex; flex-direction: column; gap: 8px; background: var(--pad-bg-color-100); border: 1px solid var(--pad-border-color-200); border-radius: var(--pad-radius-md); font-size: 13px; }
.failure { color: var(--q-negative); overflow-wrap: anywhere; }
.device-acknowledgement { color: var(--pad-danger-color); }
:global(.key-change-memory-popup) { background: var(--pad-bg-color-200); color: var(--pad-text-color-100); }
:global(.key-change-memory-popup .q-item) { color: var(--pad-text-color-100); }
@media (max-width: 480px) { .key-change-header { padding: 16px; } .key-change-content { padding: 16px; } .plan-grid { gap: 10px; font-size: 12px; } }
</style>
