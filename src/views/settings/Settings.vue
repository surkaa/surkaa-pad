<template>
  <div id="settings-page">
    <div class="settings-content">
      <div class="settings-content-inner">
        <section class="settings-group">
        <div class="group-title">外观界面</div>
        <q-card flat bordered class="pad-card">
          <q-item class="settings-item theme-heading">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="palette"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">显示模式</q-item-label>
              <q-item-label caption class="desc-text">
                {{ theme === 'system' ? '自动跟随系统外观' : theme === 'light' ? '始终使用浅色外观' : '始终使用深色外观' }}
              </q-item-label>
            </q-item-section>
          </q-item>
          <q-card-section class="theme-picker">
            <q-btn-toggle
                v-model="theme"
                spread
                no-caps
                unelevated
                class="theme-toggle"
                toggle-color="primary"
                :options="[
                {label: '跟随系统', value: 'system', icon: 'desktop_windows'},
                {label: '浅色模式', value: 'light', icon: 'light_mode'},
                {label: '深色模式', value: 'dark', icon: 'dark_mode'}
              ]"
            />
          </q-card-section>
          <q-separator/>
          <q-list>
            <q-item tag="label" v-ripple class="settings-item">
              <q-item-section avatar class="settings-icon-section">
                <q-icon name="photo_size_select_small"/>
              </q-item-section>
              <q-item-section>
                <q-item-label class="label-text text-weight-medium">默认使用小图</q-item-label>
                <q-item-label caption class="desc-text">新插入的单张图片以小图显示</q-item-label>
              </q-item-section>
              <q-item-section side>
                <q-toggle
                    v-model="defaultImageSize"
                    color="primary"
                />
              </q-item-section>
            </q-item>
          </q-list>
        </q-card>
        </section>

        <section class="settings-group">
        <div class="group-title">安全隐私</div>
        <q-list bordered separator class="pad-card">
          <q-item tag="label" v-ripple :disable="!isAndroid" class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="fingerprint"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">生物识别解锁</q-item-label>
              <q-item-label caption class="desc-text">使用指纹或面容快速解锁应用</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-toggle
                  v-model="biometricEnable"
                  @update:model-value="handleBiometricToggle"
                  color="primary"
                  :disable="!isAndroid"
              />
              <q-badge v-if="!isAndroid" color="grey-6" floating transparent style="top: 8px; right: 0;">系统不支持
              </q-badge>
            </q-item-section>
          </q-item>
        </q-list>
        </section>

        <section class="settings-group">
        <div class="group-title">附件加密</div>
        <q-list bordered separator class="pad-card">
          <q-item tag="label" v-ripple class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="image"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">图片</q-item-label>
              <q-item-label caption class="desc-text">包括选择图片和拍照</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-toggle v-model="encryptImageAttachments" color="primary"/>
            </q-item-section>
          </q-item>
          <q-item tag="label" v-ripple class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="audiotrack"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">音频</q-item-label>
              <q-item-label caption class="desc-text">包括选择音频和录音</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-toggle v-model="encryptAudioAttachments" color="primary"/>
            </q-item-section>
          </q-item>
          <q-item tag="label" v-ripple class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="video_library"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">视频</q-item-label>
              <q-item-label caption class="desc-text">控制新上传视频的加密状态</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-toggle v-model="encryptVideoAttachments" color="primary"/>
            </q-item-section>
          </q-item>
          <q-item tag="label" v-ripple class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="attach_file"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">文件</q-item-label>
              <q-item-label caption class="desc-text">控制其他新上传文件的加密状态</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-toggle v-model="encryptFileAttachments" color="primary"/>
            </q-item-section>
          </q-item>
        </q-list>
        </section>

        <section class="settings-group">
        <div class="group-title">云存储</div>
        <q-list bordered separator class="pad-card">
          <q-item class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="cloud_sync"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">云同步</q-item-label>
              <q-item-label caption class="desc-text">
                {{ remoteEnabled ? '已启用' : '未启用' }}
              </q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-toggle
                  :model-value="remoteEnabled"
                  @update:model-value="handleRemoteToggle"
                  color="primary"
                  :disable="remoteStorageBusy"
              />
            </q-item-section>
          </q-item>
        </q-list>
        </section>

        <section class="settings-group">
        <div class="group-title">数据管理</div>
        <q-list bordered separator class="pad-card">
          <q-item clickable v-ripple @click="exportLogFile" class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="description"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">导出日志文件</q-item-label>
              <q-item-label caption class="desc-text">保存诊断日志以便排查问题</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-icon name="chevron_right" class="desc-text"/>
            </q-item-section>
          </q-item>
          <q-item v-if="remoteEnabled" clickable v-ripple @click="cleanUnusedFile" class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="cleaning_services"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">清除过期缓存</q-item-label>
              <q-item-label caption class="desc-text">清理不再使用的本地附件缓存</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-icon name="chevron_right" class="desc-text"/>
            </q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="handleReset" class="settings-item danger-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="restart_alt"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="text-negative text-weight-medium">重置应用配置</q-item-label>
              <q-item-label caption class="desc-text">清除本机配置并重启应用</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-icon name="chevron_right" color="negative"/>
            </q-item-section>
          </q-item>
        </q-list>
        </section>
      </div>
    </div>

    <!-- OSS 配置对话框 -->
    <q-dialog v-model="showOssConfigDialog" persistent>
      <q-card class="pad-modal" style="min-width: 340px">
        <q-card-section>
          <div class="text-h6 title-text">配置云存储</div>
          <div class="text-caption desc-text">填写阿里云 OSS 配置以启用云同步</div>
        </q-card-section>
        <q-card-section class="q-pt-none q-gutter-y-sm">
          <q-input v-model="ossConfig.akid" label="AccessKey ID" outlined dense color="primary" :disable="remoteStorageBusy"
                   :rules="[val => !!val || '必填']" hide-bottom-space/>
          <q-input v-model="ossConfig.aks" type="password" label="AccessKey Secret" outlined dense color="primary"
                   :disable="remoteStorageBusy" :rules="[val => !!val || '必填']" hide-bottom-space/>
          <q-input v-model="ossConfig.bucket" label="Bucket 名称" outlined dense color="primary" :disable="remoteStorageBusy"
                   :rules="[val => !!val || '必填']" hide-bottom-space/>
          <q-input v-model="ossConfig.endpoint" label="Endpoint" outlined dense color="primary" :disable="remoteStorageBusy"
                   :rules="[val => !!val || '必填']" hide-bottom-space/>
        </q-card-section>
        <q-card-actions align="right" class="q-pb-md q-pr-md">
          <q-btn flat label="取消" color="grey-7" v-close-popup :disable="remoteStorageBusy"/>
          <q-btn unelevated label="启用云同步" color="primary" :loading="remoteStorageBusy" @click="doEnableRemote"/>
        </q-card-actions>
      </q-card>
    </q-dialog>

    <!-- 同步进度对话框 -->
    <q-dialog v-model="showSyncProgress" persistent>
      <q-card class="pad-modal" style="min-width: 300px">
        <q-card-section>
          <div class="text-h6 title-text">同步中</div>
        </q-card-section>
        <q-card-section class="q-pt-none">
          <div class="desc-text q-mb-sm">{{ syncStatusText }}</div>
          <div v-if="syncCurrentFile" class="text-caption ellipsis q-mb-xs">
            {{ syncCurrentFile }}
          </div>
          <div v-if="syncFileDetail" class="text-caption desc-text q-mb-sm">
            {{ syncFileDetail }}
          </div>
          <q-linear-progress
              v-if="syncTotal > 0"
              :value="Math.min(syncProgress / syncTotal, 1)"
              color="primary"
              class="q-mt-sm"
          />
          <q-spinner v-else color="primary" size="2em"/>
        </q-card-section>
      </q-card>
    </q-dialog>

    <q-dialog v-model="showPasswordVerify" persistent>
      <q-card class="pad-modal" style="min-width: 300px">
        <q-card-section>
          <div class="text-h6 title-text">验证主密码</div>
          <div class="text-caption desc-text">请输入主密码以授权生物识别解锁</div>
        </q-card-section>

        <q-card-section class="q-pt-none">
          <q-input
              dense
              outlined
              v-model="verifyPassword"
              type="password"
              placeholder="请输入主密码"
              autofocus
              @keyup.enter="confirmEnableBiometric"
          />
        </q-card-section>

        <q-card-actions align="right" class="q-pb-md q-pr-md">
          <q-btn flat label="取消" color="grey-7" v-close-popup @click="cancelBiometric"/>
          <q-btn unelevated label="确认开启" color="primary" :loading="loading" :disable="!verifyPassword"
                 @click="confirmEnableBiometric"/>
        </q-card-actions>
      </q-card>
    </q-dialog>
  </div>
</template>

<script setup lang="ts">
import {onMounted, ref} from 'vue';
import {platform} from "@tauri-apps/plugin-os";
import {confirm} from '@tauri-apps/plugin-dialog';
import {exportLogFile} from "../../utils";
import {relaunch} from '@tauri-apps/plugin-process';
import {useQuasar} from "quasar";
import {useConfigStore} from "../../stores/config.ts";
import {biometricCipher} from "@tauri-apps/plugin-biometric";
import api from "../../utils/api.ts";
import {formatError} from "../../utils/formatError.ts";
import {OssConfigType} from "../../types.ts";
import {Channel} from "@tauri-apps/api/core";
import {SyncProgressEvent} from "../../bindings.ts";
import {
  initialSyncProgressDisplay,
  reduceSyncProgressDisplay,
  type SyncProgressDisplay,
} from "../../utils/syncProgress.ts";
import {remoteStorageToggleAction} from "../../utils/remoteStorageToggle.ts";

const $q = useQuasar();
const configStore = useConfigStore();

const showPasswordVerify = ref(false);
const verifyPassword = ref('');
const loading = ref(false);
const theme = configStore.useTauriConfig('app-theme');
const biometricEnable = configStore.useTauriConfig('biometric_enabled');
const defaultImageSize = configStore.useTauriConfig('default_image_size_is_small');
const encryptImageAttachments = configStore.useTauriConfig('encrypt_image_attachments');
const encryptAudioAttachments = configStore.useTauriConfig('encrypt_audio_attachments');
const encryptVideoAttachments = configStore.useTauriConfig('encrypt_video_attachments');
const encryptFileAttachments = configStore.useTauriConfig('encrypt_file_attachments');
const isAndroid = ref(platform() === 'android');

// 云存储
const remoteEnabled = ref(false);
const remoteStorageBusy = ref(false);
const showOssConfigDialog = ref(false);
const showSyncProgress = ref(false);
const syncProgress = ref(0);
const syncTotal = ref(0);
const syncStatusText = ref('');
const syncCurrentFile = ref('');
const syncFileDetail = ref('');
const ossConfig = ref<OssConfigType>({akid: '', aks: '', bucket: '', endpoint: ''});

onMounted(async () => {
  remoteEnabled.value = await api.cmdGetStorageMode();
});

function resetSyncProgress(status: string) {
  applySyncProgressDisplay(initialSyncProgressDisplay(status));
}

function handleSyncProgressEvent(msg: SyncProgressEvent) {
  applySyncProgressDisplay(reduceSyncProgressDisplay({
    progress: syncProgress.value,
    total: syncTotal.value,
    statusText: syncStatusText.value,
    currentFile: syncCurrentFile.value,
    fileDetail: syncFileDetail.value,
  }, msg));
}

function applySyncProgressDisplay(display: SyncProgressDisplay) {
  syncProgress.value = display.progress;
  syncTotal.value = display.total;
  syncStatusText.value = display.statusText;
  syncCurrentFile.value = display.currentFile;
  syncFileDetail.value = display.fileDetail;
}

async function handleRemoteToggle(newValue: boolean) {
  const action = remoteStorageToggleAction(
      remoteEnabled.value,
      newValue,
      remoteStorageBusy.value,
  );
  if (action === 'enable') {
    // 开启：弹出 OSS 配置对话框
    showOssConfigDialog.value = true;
  } else if (action === 'disable') {
    // 关闭
    remoteStorageBusy.value = true;
    try {
      if (await confirm('关闭云同步后，云端数据将下载到本地。确定继续？')) {
        await doDisableRemote();
      }
    } finally {
      remoteStorageBusy.value = false;
    }
  }
}

async function doEnableRemote() {
  const {akid, aks, bucket, endpoint} = ossConfig.value;
  if (!akid || !aks || !bucket || !endpoint) {
    $q.notify({type: 'warning', message: '请填写完整的 OSS 配置'});
    return;
  }
  remoteStorageBusy.value = true;
  showOssConfigDialog.value = false;
  showSyncProgress.value = true;
  resetSyncProgress('正在同步数据到云端...');

  try {
    const encryptedConfig = await api.cmdEncryptData(JSON.stringify(ossConfig.value));
    // 先明确记录尚未启用，避免只有加密配置时触发旧版本兼容逻辑。
    await configStore.saveNormalConfig('remote_enabled', false);
    await configStore.saveNormalConfig('encrypted_oss_config', encryptedConfig);

    const event = new Channel<SyncProgressEvent>();
    event.onmessage = handleSyncProgressEvent;

    await api.cmdEnableRemoteStorage(event, akid, aks, bucket, endpoint);
    await configStore.saveNormalConfig('remote_enabled', true);
    remoteEnabled.value = true;
    $q.notify({type: 'positive', message: '云同步已启用'});
  } catch (e) {
    $q.notify({type: 'negative', message: `启用云同步失败: ${formatError(e)}`});
    // 同步可能只是暂时的网络错误。保留已加密配置与当前表单，直接回到
    // 配置对话框即可重试；后端仍保持本地模式。
    showOssConfigDialog.value = true;
  } finally {
    showSyncProgress.value = false;
    remoteStorageBusy.value = false;
  }
}

async function doDisableRemote() {
  showSyncProgress.value = true;
  resetSyncProgress('正在从云端下载数据...');

  try {
    const event = new Channel<SyncProgressEvent>();
    event.onmessage = handleSyncProgressEvent;

    await api.cmdDisableRemoteStorage(event);
    await configStore.saveNormalConfig('remote_enabled', false);
    remoteEnabled.value = false;
    $q.notify({type: 'positive', message: '云同步已关闭，数据已下载到本地'});
  } catch (e) {
    $q.notify({type: 'negative', message: `关闭云同步失败: ${formatError(e)}`});
  } finally {
    showSyncProgress.value = false;
  }
}

// 接收 Quasar v-model 抛出的 boolean
async function handleBiometricToggle(newValue: boolean) {
  if (newValue) {
    showPasswordVerify.value = true;
    verifyPassword.value = '';
  } else {
    if (await confirm('确定要关闭生物识别解锁吗？')) {
      await configStore.deleteConfig('biometric_enabled', 'biometric_dek');
      $q.notify('生物识别已禁用');
    }
  }
}

async function confirmEnableBiometric() {
  if (!verifyPassword.value) return;
  loading.value = true;
  try {
    const dataToEncrypt = await api.cmdValidPassword(verifyPassword.value);
    const response = await biometricCipher('请验证生物识别以启用快速解锁', {dataToEncrypt});
    await configStore.saveNormalConfig('biometric_enabled', true);
    await configStore.saveNormalConfig('biometric_dek', response.data);
    $q.notify('生物识别已成功开启');
    showPasswordVerify.value = false;
  } catch (err: any) {
    $q.notify({type: 'negative', message: formatError(err)});
  } finally {
    loading.value = false;
  }
}

function cancelBiometric() {
  showPasswordVerify.value = false;
  verifyPassword.value = '';
}

async function handleReset() {
  if (await confirm('确定要重置应用配置吗？此操作不可撤销。重置后将自动重启应用')) {
    await configStore.deleteConfig(
        'encrypted_oss_config',
        'remote_enabled',
        'biometric_dek',
        'biometric_enabled',
        'encrypt_image_attachments',
        'encrypt_audio_attachments',
        'encrypt_video_attachments',
        'encrypt_file_attachments',
    );
    await api.cmdCleanCacheFile();
    $q.notify('配置已重置, 即将自动重启');
    setTimeout(relaunch, 1000);
  }
}

async function cleanUnusedFile() {
  try {
    const deleted = await api.cmdCleanUnusedFile();
    $q.notify({type: 'positive', message: `清除了${deleted.length}个缓存文件`});
  } catch (e) {
    $q.notify({type: 'negative', message: formatError(e)});
  }
}

defineOptions({name: 'Settings'});
</script>

<style scoped lang="scss">
#settings-page {
  width: 100%;
  height: 100%;
  background-color: var(--pad-bg-color-100);
  display: flex;
  flex-direction: column;

  .settings-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    width: 100%;
  }

  .settings-content-inner {
    width: 100%;
    padding: 18px 20px 32px;
    box-sizing: border-box;
    text-align: left;
  }

  .settings-group {
    margin-bottom: 22px;
  }

  .title-text {
    color: var(--pad-text-color-100);
  }

  .label-text {
    color: var(--pad-text-color-200);
  }

  .desc-text {
    color: var(--pad-text-color-400);
  }

  .group-title {
    margin: 0 0 8px 12px;
    font-size: 0.8rem;
    font-weight: 500;
    color: var(--pad-text-color-400);
    letter-spacing: 0.08em;
  }

  .pad-card {
    background-color: var(--pad-bg-color-200);
    border-color: var(--pad-border-color-100);
    border-radius: 14px;
    overflow: hidden;
    box-shadow: 0 2px 10px var(--pad-shadow-color-100);
  }

  .settings-item {
    min-height: 66px;
    padding: 10px 16px;

    :deep(.q-item__section:not(.q-item__section--side)) {
      align-items: flex-start;
      text-align: left;
    }

    :deep(.q-item__section--side) {
      padding-left: 12px;
    }
  }

  .settings-icon-section {
    min-width: 42px;
    padding-right: 14px;

    .q-icon {
      width: 34px;
      height: 34px;
      border-radius: 10px;
      background: color-mix(in srgb, var(--pad-primary-color) 16%, transparent);
      color: var(--pad-primary-dark);
      font-size: 20px;
    }
  }

  .danger-item .settings-icon-section .q-icon {
    background: color-mix(in srgb, var(--pad-danger-color) 14%, transparent);
    color: var(--pad-danger-color);
  }

  .theme-heading {
    min-height: 60px;
    padding-bottom: 4px;
  }

  .theme-picker {
    padding: 8px 16px 16px 72px;
    text-align: left;
  }

  .pad-modal {
    background-color: var(--pad-bg-color-100);
    border-radius: var(--pad-radius-xl);
  }

  .theme-toggle {
    width: 100%;
    border: 1px solid var(--pad-border-color-100);
    background-color: var(--pad-bg-color-100);
    border-radius: 10px;
    overflow: hidden;

    :deep(.q-btn) {
      color: var(--pad-text-color-300);
      min-height: 42px;
    }
  }

  @media (max-width: 600px) {
    .settings-content-inner {
      padding: 14px 12px 28px;
    }

    .settings-group {
      margin-bottom: 18px;
    }

    .settings-item {
      padding-right: 12px;
      padding-left: 12px;
    }

    .settings-icon-section {
      min-width: 38px;
      padding-right: 10px;
    }

    .theme-picker {
      padding-right: 12px;
      padding-left: 60px;
    }

    .theme-toggle :deep(.q-btn) {
      padding: 4px;
      font-size: 12px;
    }
  }
}
</style>
