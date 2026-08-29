<script setup lang="ts">
import {Channel} from '@tauri-apps/api/core';
import {relaunch} from '@tauri-apps/plugin-process';
import {platform} from "@tauri-apps/plugin-os";
import {Dark} from 'quasar';
import {useEventListener} from "@vueuse/core";
import {useConfigStore} from "./stores/config.ts";
import {onMounted, ref, watchEffect} from "vue";
import type {LocalStorageMigrationEvent} from './bindings';
import LocalStorageMigrationDialog from './components/LocalStorageMigrationDialog.vue';
import api from './utils/api';
import {formatError} from './utils/formatError';
import {
  initialLocalStorageMigrationDisplay,
  reduceLocalStorageMigrationDisplay,
  withLocalStorageMigrationError,
} from './utils/localStorageMigration';
import {logStartupError, logStartupPhase} from './utils/startupLog';
import AndroidShareImportCoordinator from './components/AndroidShareImportCoordinator.vue';
import {resolveQuasarDarkMode} from './utils/theme';

const configStore = useConfigStore();
const p = platform();
const showLocalStorageMigration = ref(false);
const localStorageMigrationDisplay = ref(initialLocalStorageMigrationDisplay());
const showUnavailableLocalStorage = ref(false);
const unavailableLocalStoragePath = ref('');
const unavailableLocalStorageReason = ref('');
const checkingLocalStorage = ref(false);

if (p === 'windows') {
  useEventListener('keydown', (event: KeyboardEvent) => {
    // 阻止 F5 键
    if (event.key === 'F5') {
      console.log('刷新已被禁用');
      event.preventDefault();
      return;
    }

    // 阻止普通的 Ctrl+R 刷新，带 Alt/Shift 的组合保留给编辑器快捷键
    if (
      (event.ctrlKey || event.metaKey)
      && !event.altKey
      && !event.shiftKey
      && event.key.toLowerCase() === 'r'
    ) {
      console.log('刷新已被禁用');
      event.preventDefault();
      return;
    }
  });

  useEventListener('contextmenu', (e) => {
    e.preventDefault();
  });
}

const theme = configStore.useTauriConfig('app-theme');

// 必须同步 Quasar 自身的暗色状态。仅切换 body--dark 类只能更新项目 CSS
// 变量，QInput、QSelect 等组件仍会按浅色模式渲染，Teleport 到根节点的
// QDialog 内容尤其容易暴露这个问题。
watchEffect(() => {
  Dark.set(resolveQuasarDarkMode(theme.value));
});

onMounted(() => {
  logStartupPhase('Root mounted');
  void checkStartupLocalStorageMigration();
});

async function checkStartupLocalStorageMigration() {
  logStartupPhase('Local storage startup check started');
  checkingLocalStorage.value = true;
  try {
    const status = await api.cmdGetLocalStorageMigrationStatus();
    if (status.unavailablePath) {
      unavailableLocalStoragePath.value = status.unavailablePath;
      unavailableLocalStorageReason.value = status.unavailableReason ?? '目录不可访问';
      showUnavailableLocalStorage.value = true;
      return;
    }
    showUnavailableLocalStorage.value = false;
    if (status.migrationPending) {
      await resumeLocalStorageMigration();
    }
  } catch (error) {
    logStartupError('Local storage startup check failed', error);
    console.error('检查本地数据迁移状态失败:', error);
  } finally {
    checkingLocalStorage.value = false;
    logStartupPhase('Local storage startup check completed');
  }
}

async function resumeLocalStorageMigration() {
  showLocalStorageMigration.value = true;
  localStorageMigrationDisplay.value = initialLocalStorageMigrationDisplay();
  const event = new Channel<LocalStorageMigrationEvent>();
  event.onmessage = (message) => {
    localStorageMigrationDisplay.value = reduceLocalStorageMigrationDisplay(
      localStorageMigrationDisplay.value,
      message,
    );
  };

  try {
    await api.cmdMigrateLocalStorage(event, null);
  } catch (error) {
    if (!localStorageMigrationDisplay.value.error) {
      localStorageMigrationDisplay.value = withLocalStorageMigrationError(
        localStorageMigrationDisplay.value,
        formatError(error),
      );
    }
  }
}
</script>

<template>
  <router-view v-slot="{ Component }">
    <component :is="Component"/>
  </router-view>
  <AndroidShareImportCoordinator/>
  <LocalStorageMigrationDialog
    v-model="showLocalStorageMigration"
    title="继续迁移本地数据位置"
    :display="localStorageMigrationDisplay"
    allow-defer
    @retry="resumeLocalStorageMigration"
    @defer="showLocalStorageMigration = false"
    @restart="relaunch"
  />
  <q-dialog :model-value="showUnavailableLocalStorage" persistent>
    <q-card class="storage-unavailable-dialog">
      <q-card-section>
        <div class="text-h6 dialog-title">本地数据位置不可用</div>
        <div class="text-caption dialog-description">
          请重新连接对应磁盘或恢复目录访问权限，然后重新检测。为避免产生一套新的空数据，恢复前不能继续使用应用。
        </div>
      </q-card-section>
      <q-card-section class="q-pt-none">
        <div class="path-text">{{ unavailableLocalStoragePath }}</div>
        <div class="text-caption error-text q-mt-sm">{{ unavailableLocalStorageReason }}</div>
      </q-card-section>
      <q-card-actions align="right" class="q-px-md q-pb-md">
        <q-btn
          unelevated
          label="重新检测"
          color="primary"
          :loading="checkingLocalStorage"
          @click="checkStartupLocalStorageMigration"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<style scoped lang="scss">
.storage-unavailable-dialog {
  width: min(440px, calc(100vw - 32px));
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-100);
  border-radius: var(--pad-radius-xl);
}

.dialog-title,
.path-text {
  color: var(--pad-text-color-200);
}

.dialog-description {
  color: var(--pad-text-color-400);
}

.path-text {
  overflow-wrap: anywhere;
  font-size: 0.88rem;
}

.error-text {
  color: var(--pad-danger-color);
}
</style>
