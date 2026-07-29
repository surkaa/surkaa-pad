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

        <section v-if="isAndroid" class="settings-group">
        <div class="group-title">安全隐私</div>
        <q-list bordered separator class="pad-card">
          <q-item tag="label" v-ripple class="settings-item">
            <q-item-section avatar class="settings-icon-section">
              <q-icon name="fingerprint"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="label-text text-weight-medium">生物识别解锁</q-item-label>
              <q-item-label caption class="desc-text">使用指纹或面容快速解锁，每 7 天需验证一次主密码</q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-toggle
                  :model-value="biometricEnable"
                  @update:model-value="handleBiometricToggle"
                  color="primary"
                  :disable="loading"
              />
            </q-item-section>
          </q-item>
        </q-list>
        </section>

        <AttachmentSettings/>

        <EditorShortcutSettings v-if="isWindows"/>

        <section class="settings-group">
        <div class="group-title">数据管理</div>
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
          <LocalStorageSettings v-if="isWindows"/>
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
          <q-btn unelevated label="启用云同步" color="primary" :loading="remoteStorageBusy" @click="enableRemote"/>
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
import {ref} from 'vue';
import {platform} from "@tauri-apps/plugin-os";
import {confirm} from '@tauri-apps/plugin-dialog';
import {exportLogFile} from "../../utils";
import {useQuasar} from "quasar";
import {useConfigStore} from "../../stores/config.ts";
import {biometricCipher} from "../../utils/biometric.ts";
import api from "../../utils/api.ts";
import {formatError} from "../../utils/formatError.ts";
import {biometricToggleAction} from "../../utils/biometricToggle.ts";
import {useRemoteStorageSettings} from '../../composables/useRemoteStorageSettings';
import AttachmentSettings from './AttachmentSettings.vue';
import EditorShortcutSettings from './EditorShortcutSettings.vue';
import LocalStorageSettings from './LocalStorageSettings.vue';

const $q = useQuasar();
const configStore = useConfigStore();

const showPasswordVerify = ref(false);
const verifyPassword = ref('');
const loading = ref(false);
const theme = configStore.useTauriConfig('app-theme');
const biometricEnable = configStore.useTauriConfig('biometric_enabled');
const defaultImageSize = configStore.useTauriConfig('default_image_size_is_small');
const currentPlatform = platform();
const isAndroid = ref(currentPlatform === 'android');
const isWindows = currentPlatform === 'windows';

const {
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
} = useRemoteStorageSettings();

// 接收 Quasar v-model 抛出的 boolean
async function handleBiometricToggle(newValue: boolean) {
  const action = biometricToggleAction(biometricEnable.value, newValue, loading.value);
  if (action === 'enable') {
    showPasswordVerify.value = true;
    verifyPassword.value = '';
  } else if (action === 'disable') {
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
    await configStore.saveNormalConfig('biometric_dek', response.data);
    await configStore.saveNormalConfig('last_password_unlock_at', Date.now());
    // 必须最后写入启用标记，避免密码验证或生物凭据配置未完成时开关已开启。
    await configStore.saveNormalConfig('biometric_enabled', true);
    $q.notify('生物识别已成功开启');
    showPasswordVerify.value = false;
    verifyPassword.value = '';
  } catch (err: any) {
    try {
      await configStore.deleteConfig('biometric_enabled', 'biometric_dek');
    } catch (cleanupError) {
      console.error('清理未完成的生物识别配置失败:', cleanupError);
    }
    $q.notify({type: 'negative', message: formatError(err)});
  } finally {
    loading.value = false;
  }
}

function cancelBiometric() {
  showPasswordVerify.value = false;
  verifyPassword.value = '';
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
    padding: 10px 12px;

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
    padding-right: 10px;

    .q-icon {
      width: 34px;
      height: 34px;
      border-radius: 10px;
      background: color-mix(in srgb, var(--pad-primary-color) 16%, transparent);
      color: var(--pad-primary-dark);
      font-size: 20px;
    }
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
