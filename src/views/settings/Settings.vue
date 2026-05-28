<template>
  <div id="settings-page">
    <div class="settings-content q-pa-md">
      <div class="q-mb-lg">
        <div class="group-title q-mb-sm">外观界面</div>
        <q-card flat bordered class="pad-card rounded-borders">
          <q-card-section>
            <div class="text-weight-medium q-mb-md label-text">显示模式</div>
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
            <q-item tag="label" v-ripple>
              <q-item-section>
                <q-item-label class="label-text text-weight-medium">默认使用小图</q-item-label>
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
      </div>

      <div class="q-mb-lg">
        <div class="group-title q-mb-sm">安全隐私</div>
        <q-list bordered separator class="pad-card rounded-borders">
          <q-item tag="label" v-ripple :disable="!isAndroid">
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
      </div>

      <div class="q-mb-lg">
        <div class="group-title q-mb-sm">云存储</div>
        <q-list bordered separator class="pad-card rounded-borders">
          <q-item>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">云同步</q-item-label>
              <q-item-label caption class="desc-text">
                {{ remoteEnabled ? '已启用' : '未启用' }}
              </q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-toggle
                  v-model="remoteEnabled"
                  @update:model-value="handleRemoteToggle"
                  color="primary"
              />
            </q-item-section>
          </q-item>
        </q-list>
      </div>

      <div class="q-mb-lg">
        <div class="group-title q-mb-sm">数据管理</div>
        <q-list bordered separator class="pad-card rounded-borders">
          <q-item clickable v-ripple @click="exportLogFile">
            <q-item-section class="label-text text-weight-medium">导出日志文件</q-item-section>
            <q-item-section side>
              <q-icon name="chevron_right" class="desc-text"/>
            </q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="cleanUnusedFile">
            <q-item-section class="label-text text-weight-medium">清除过期缓存</q-item-section>
            <q-item-section side>
              <q-icon name="chevron_right" class="desc-text"/>
            </q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="handleReset">
            <q-item-section class="text-negative text-weight-medium">重置应用配置</q-item-section>
            <q-item-section side>
              <q-icon name="chevron_right" color="negative"/>
            </q-item-section>
          </q-item>
        </q-list>
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
          <q-input v-model="ossConfig.akid" label="AccessKey ID" outlined dense color="primary" :disable="loading"
                   :rules="[val => !!val || '必填']" hide-bottom-space/>
          <q-input v-model="ossConfig.aks" type="password" label="AccessKey Secret" outlined dense color="primary"
                   :disable="loading" :rules="[val => !!val || '必填']" hide-bottom-space/>
          <q-input v-model="ossConfig.bucket" label="Bucket 名称" outlined dense color="primary" :disable="loading"
                   :rules="[val => !!val || '必填']" hide-bottom-space/>
          <q-input v-model="ossConfig.endpoint" label="Endpoint" outlined dense color="primary" :disable="loading"
                   :rules="[val => !!val || '必填']" hide-bottom-space/>
        </q-card-section>
        <q-card-actions align="right" class="q-pb-md q-pr-md">
          <q-btn flat label="取消" color="grey-7" v-close-popup @click="remoteEnabled = false"/>
          <q-btn unelevated label="启用云同步" color="primary" :loading="loading" @click="doEnableRemote"/>
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
          <q-linear-progress
              v-if="syncTotal > 0"
              :value="syncTotal > 0 ? syncProgress / syncTotal : 0"
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

const $q = useQuasar();
const configStore = useConfigStore();

const showPasswordVerify = ref(false);
const verifyPassword = ref('');
const loading = ref(false);
const theme = configStore.useTauriConfig('app-theme');
const biometricEnable = configStore.useTauriConfig('biometric_enabled');
const defaultImageSize = configStore.useTauriConfig('default_image_size_is_small');
const isAndroid = ref(platform() === 'android');

// 云存储
const remoteEnabled = ref(false);
const skipRemoteToggleHandler = ref(false);
const showOssConfigDialog = ref(false);
const showSyncProgress = ref(false);
const syncProgress = ref(0);
const syncTotal = ref(0);
const syncStatusText = ref('');
const ossConfig = ref<OssConfigType>({akid: '', aks: '', bucket: '', endpoint: ''});

onMounted(async () => {
  remoteEnabled.value = await api.cmdGetStorageMode();
});

async function handleRemoteToggle(newValue: boolean) {
  if (skipRemoteToggleHandler.value) {
    skipRemoteToggleHandler.value = false;
    return;
  }
  if (newValue) {
    // 开启：弹出 OSS 配置对话框
    ossConfig.value = {akid: '', aks: '', bucket: '', endpoint: ''};
    showOssConfigDialog.value = true;
  } else {
    // 关闭
    if (await confirm('关闭云同步后，云端数据将下载到本地。确定继续？')) {
      await doDisableRemote();
    } else {
      remoteEnabled.value = true;
    }
  }
}

async function doEnableRemote() {
  const {akid, aks, bucket, endpoint} = ossConfig.value;
  if (!akid || !aks || !bucket || !endpoint) {
    $q.notify({type: 'warning', message: '请填写完整的 OSS 配置'});
    return;
  }
  showOssConfigDialog.value = false;
  showSyncProgress.value = true;
  syncStatusText.value = '正在同步数据到云端...';
  syncProgress.value = 0;

  try {
    const encryptedConfig = await api.cmdEncryptData(JSON.stringify(ossConfig.value));
    await configStore.saveNormalConfig('encrypted_oss_config', encryptedConfig);

    const event = new Channel<SyncProgressEvent>();
    event.onmessage = (msg: any) => {
      if (msg.event === 'started') {
        syncTotal.value = msg.data.total;
      } else if (msg.event === 'progress') {
        syncProgress.value = msg.data.current;
        syncTotal.value = msg.data.total;
        syncStatusText.value = `正在同步 ${msg.data.current}/${msg.data.total}...`;
      } else if (msg.event === 'completed') {
        syncStatusText.value = '同步完成';
      }
    };

    await api.cmdEnableRemoteStorage(event, akid, aks, bucket, endpoint);
    await configStore.saveNormalConfig('remote_enabled', true);
    skipRemoteToggleHandler.value = true;
    remoteEnabled.value = true;
    $q.notify({type: 'positive', message: '云同步已启用'});
  } catch (e) {
    $q.notify({type: 'negative', message: `启用云同步失败: ${formatError(e)}`});
    skipRemoteToggleHandler.value = true;
    remoteEnabled.value = false;
    await configStore.deleteConfig('encrypted_oss_config');
  } finally {
    showSyncProgress.value = false;
  }
}

async function doDisableRemote() {
  showSyncProgress.value = true;
  syncStatusText.value = '正在从云端下载数据...';
  syncProgress.value = 0;

  try {
    const event = new Channel<SyncProgressEvent>();
    event.onmessage = (msg: any) => {
      if (msg.event === 'started') {
        syncTotal.value = msg.data.total;
      } else if (msg.event === 'progress') {
        syncProgress.value = msg.data.current;
        syncTotal.value = msg.data.total;
        syncStatusText.value = `正在下载 ${msg.data.current}/${msg.data.total}...`;
      } else if (msg.event === 'completed') {
        syncStatusText.value = '下载完成';
      }
    };

    await api.cmdDisableRemoteStorage(event);
    await configStore.saveNormalConfig('remote_enabled', false);
    skipRemoteToggleHandler.value = true;
    remoteEnabled.value = false;
    $q.notify({type: 'positive', message: '云同步已关闭，数据已下载到本地'});
  } catch (e) {
    $q.notify({type: 'negative', message: `关闭云同步失败: ${formatError(e)}`});
    skipRemoteToggleHandler.value = true;
    remoteEnabled.value = true;
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
    await configStore.deleteConfig('encrypted_oss_config', 'remote_enabled', 'biometric_dek', 'biometric_enabled');
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
    overflow-y: auto;
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
    font-size: 0.85rem;
    color: var(--pad-text-color-400);
    padding-left: 8px;
    letter-spacing: 1px;
  }

  .pad-card {
    background-color: var(--pad-bg-color-200);
    border-color: var(--pad-border-color-100);
  }

  .pad-modal {
    background-color: var(--pad-bg-color-100);
    border-radius: var(--pad-radius-xl);
  }

  .theme-toggle {
    border: 1px solid var(--pad-border-color-100);
    background-color: var(--pad-bg-color-100);

    :deep(.q-btn) {
      color: var(--pad-text-color-300);
    }
  }
}
</style>
