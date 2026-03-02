<template>
  <div id="settings-page">
    <div class="settings-content q-pa-md">
      <div class="q-mb-lg">
        <div class="group-title q-mb-sm">外观界面</div>
        <q-card flat bordered class="pad-card rounded-borders">
          <q-card-section>
            <div class="text-weight-medium q-mb-md label-text">显示模式</div>
            <q-btn-toggle
                v-model="appStore.theme"
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
                @update:model-value="val => appStore.setTheme(val)"
            />
          </q-card-section>
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
                  :model-value="appStore.isBiometricEnabled"
                  @update:model-value="handleBiometricToggle"
                  color="primary"
                  :disable="!isAndroid"
              />
              <q-badge v-if="!isAndroid" color="grey-6" floating transparent style="top: 8px; right: 0;">系统不支持</q-badge>
            </q-item-section>
          </q-item>
        </q-list>
      </div>

      <div class="q-mb-lg">
        <div class="group-title q-mb-sm">数据管理</div>
        <q-list bordered separator class="pad-card rounded-borders">
          <q-item clickable v-ripple @click="exportLogFile">
            <q-item-section class="label-text text-weight-medium">导出日志文件</q-item-section>
            <q-item-section side><q-icon name="chevron_right" class="desc-text" /></q-item-section>
          </q-item>
          <q-item clickable v-ripple @click="handleReset">
            <q-item-section class="text-negative text-weight-medium">重置应用配置</q-item-section>
            <q-item-section side><q-icon name="chevron_right" color="negative" /></q-item-section>
          </q-item>
        </q-list>
      </div>
    </div>

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
          <q-btn flat label="取消" color="grey-7" v-close-popup @click="cancelBiometric" />
          <q-btn unelevated label="确认开启" color="primary" :loading="loading" :disable="!verifyPassword" @click="confirmEnableBiometric" />
        </q-card-actions>
      </q-card>
    </q-dialog>
  </div>
</template>

<script setup lang="ts">
import {ref} from 'vue';
import {useAppStore} from "../../stores/app.ts";
import {platform} from "@tauri-apps/plugin-os";
import {confirm} from '@tauri-apps/plugin-dialog';
import {exportLogFile} from "../../utils/exportLogFile.ts";
import {relaunch} from '@tauri-apps/plugin-process';
import {useQuasar} from "quasar";

const $q = useQuasar();
const appStore = useAppStore();

const showPasswordVerify = ref(false);
const verifyPassword = ref('');
const loading = ref(false);
const isAndroid = ref(platform() === 'android');

// 接收 Quasar v-model 抛出的 boolean
async function handleBiometricToggle(newValue: boolean) {
  if (newValue) {
    showPasswordVerify.value = true;
    verifyPassword.value = '';
  } else {
    if (await confirm('确定要关闭生物识别解锁吗？')) {
      await appStore.disableBiometric();
      $q.notify('生物识别已禁用');
    }
  }
}

async function confirmEnableBiometric() {
  if (!verifyPassword.value) return;
  loading.value = true;
  try {
    await appStore.enableBiometric(verifyPassword.value);
    $q.notify('生物识别已成功开启');
    showPasswordVerify.value = false;
  } catch (err: any) {
    $q.notify(`验证失败: ${err.message || err}`);
  } finally {
    loading.value = false;
  }
}

function cancelBiometric() {
  showPasswordVerify.value = false;
  verifyPassword.value = '';
}

async function handleReset() {
  if (await confirm('确定要重置所有配置吗？此操作不可撤销。重置后将自动重启应用')) {
    appStore.resetConfig().then(() => {
      $q.notify('配置已重置');
      setTimeout(relaunch, 1000);
    });
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

  .title-text { color: var(--pad-text-color-100); }
  .label-text { color: var(--pad-text-color-200); }
  .desc-text { color: var(--pad-text-color-400); }

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
