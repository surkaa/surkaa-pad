import {Channel} from '@tauri-apps/api/core';
import {confirm} from '@tauri-apps/plugin-dialog';
import {useQuasar} from 'quasar';
import {onMounted, ref} from 'vue';
import type {SyncProgressEvent} from '../bindings';
import {useConfigStore} from '../stores/config';
import {useDataStore} from '../stores/data';
import type {OssConfigType} from '../types';
import api from '../utils/api';
import {formatError} from '../utils/formatError';
import {remoteStorageToggleAction} from '../utils/remoteStorageToggle';
import {
  initialSyncProgressDisplay,
  reduceSyncProgressDisplay,
  type SyncProgressDisplay,
} from '../utils/syncProgress';

export function useRemoteStorageSettings() {
  const $q = useQuasar();
  const configStore = useConfigStore();
  const dataStore = useDataStore();
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

  function applySyncProgressDisplay(display: SyncProgressDisplay) {
    syncProgress.value = display.progress;
    syncTotal.value = display.total;
    syncStatusText.value = display.statusText;
    syncCurrentFile.value = display.currentFile;
    syncFileDetail.value = display.fileDetail;
  }

  function resetSyncProgress(status: string) {
    applySyncProgressDisplay(initialSyncProgressDisplay(status));
  }

  function handleSyncProgressEvent(message: SyncProgressEvent) {
    applySyncProgressDisplay(reduceSyncProgressDisplay({
      progress: syncProgress.value,
      total: syncTotal.value,
      statusText: syncStatusText.value,
      currentFile: syncCurrentFile.value,
      fileDetail: syncFileDetail.value,
    }, message));
  }

  async function disableRemote() {
    showSyncProgress.value = true;
    resetSyncProgress('正在从云端下载数据...');

    try {
      const event = new Channel<SyncProgressEvent>();
      event.onmessage = handleSyncProgressEvent;

      await api.cmdDisableRemoteStorage(event);
      await configStore.saveNormalConfig('remote_enabled', false);
      remoteEnabled.value = false;
      dataStore.invalidateDiaryList();
      $q.notify({type: 'positive', message: '云同步已关闭，数据已下载到本地'});
    } catch (error) {
      $q.notify({type: 'negative', message: `关闭云同步失败: ${formatError(error)}`});
    } finally {
      showSyncProgress.value = false;
    }
  }

  async function handleRemoteToggle(newValue: boolean) {
    const action = remoteStorageToggleAction(
      remoteEnabled.value,
      newValue,
      remoteStorageBusy.value,
    );
    if (action === 'enable') {
      showOssConfigDialog.value = true;
    } else if (action === 'disable') {
      remoteStorageBusy.value = true;
      try {
        if (await confirm('关闭云同步后，云端数据将下载到本地。确定继续？')) {
          await disableRemote();
        }
      } finally {
        remoteStorageBusy.value = false;
      }
    }
  }

  async function enableRemote() {
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
      await configStore.saveNormalConfig('remote_enabled', false);
      await configStore.saveNormalConfig('encrypted_oss_config', encryptedConfig);

      const event = new Channel<SyncProgressEvent>();
      event.onmessage = handleSyncProgressEvent;

      await api.cmdEnableRemoteStorage(event, akid, aks, bucket, endpoint);
      await configStore.saveNormalConfig('remote_enabled', true);
      remoteEnabled.value = true;
      dataStore.invalidateDiaryList();
      $q.notify({type: 'positive', message: '云同步已启用'});
    } catch (error) {
      $q.notify({type: 'negative', message: `启用云同步失败: ${formatError(error)}`});
      showOssConfigDialog.value = true;
    } finally {
      showSyncProgress.value = false;
      remoteStorageBusy.value = false;
    }
  }

  return {
    remoteEnabled,
    remoteStorageBusy,
    showOssConfigDialog,
    showSyncProgress,
    syncProgress,
    syncTotal,
    syncStatusText,
    syncCurrentFile,
    syncFileDetail,
    ossConfig,
    handleRemoteToggle,
    enableRemote,
  };
}
