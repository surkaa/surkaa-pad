<template>
  <section class="settings-group settings-section-component">
    <div class="group-title">本地存储</div>
    <q-list bordered separator class="pad-card">
      <q-item
        clickable
        v-ripple
        class="settings-item"
        :disable="loadingInfo || !info"
        @click="openDataLocation"
      >
        <q-item-section avatar class="settings-icon-section">
          <q-icon name="folder_open"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium">打开本地数据位置</q-item-label>
          <q-item-label caption class="desc-text">
            本地模式保存完整数据，云同步模式用作本地缓存
          </q-item-label>
          <q-item-label caption class="desc-text storage-summary">
            <template v-if="info">
              {{ info.totalFiles }} 个文件 · {{ formatBytes(info.totalBytes) }}
              <span class="path-separator">·</span>
              <span class="storage-path">{{ info.currentPath }}</span>
            </template>
            <template v-else>正在读取本地数据位置…</template>
          </q-item-label>
        </q-item-section>
        <q-item-section side>
          <q-spinner v-if="loadingInfo" color="primary" size="20px"/>
          <q-icon v-else name="open_in_new" class="desc-text"/>
        </q-item-section>
      </q-item>

      <q-item clickable v-ripple class="settings-item" @click="chooseLocation">
        <q-item-section avatar class="settings-icon-section">
          <q-icon name="drive_file_move"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium">更改本地数据位置</q-item-label>
          <q-item-label caption class="desc-text">选择新目录并安全迁移现有数据</q-item-label>
        </q-item-section>
        <q-item-section side>
          <q-icon name="chevron_right" class="desc-text"/>
        </q-item-section>
      </q-item>

      <q-item
        v-if="info && !info.isDefault"
        clickable
        v-ripple
        class="settings-item"
        @click="planMigration(null)"
      >
        <q-item-section avatar class="settings-icon-section">
          <q-icon name="settings_backup_restore"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium">恢复默认位置</q-item-label>
          <q-item-label caption class="desc-text">将本地数据迁回应用默认数据目录</q-item-label>
        </q-item-section>
        <q-item-section side>
          <q-icon name="chevron_right" class="desc-text"/>
        </q-item-section>
      </q-item>
    </q-list>

    <q-dialog v-model="showPlan" persistent>
      <q-card class="plan-dialog">
        <q-card-section>
          <div class="text-h6 dialog-title">迁移本地数据</div>
          <div class="text-caption dialog-description">
            迁移期间将暂停日记写入和附件上传，完成后需要重启应用。
          </div>
        </q-card-section>

        <q-card-section v-if="plan" class="q-pt-none plan-details">
          <div class="detail-row">
            <span>数据规模</span>
            <strong>{{ plan.totalFiles }} 个文件 · {{ formatBytes(plan.totalBytes) }}</strong>
          </div>
          <div class="detail-row">
            <span>迁移方式</span>
            <strong>{{ plan.fastMove ? '同磁盘快速移动' : '流式复制并校验' }}</strong>
          </div>
          <div v-if="!plan.fastMove" class="detail-row">
            <span>所需 / 可用</span>
            <strong>{{ formatBytes(plan.requiredBytes) }} / {{ formatBytes(plan.availableBytes) }}</strong>
          </div>
          <div class="path-detail">
            <span>从</span>
            <strong>{{ plan.sourcePath }}</strong>
          </div>
          <div class="path-detail">
            <span>到</span>
            <strong>{{ plan.targetPath }}</strong>
          </div>
        </q-card-section>

        <q-card-actions align="right" class="q-px-md q-pb-md">
          <q-btn flat label="取消" class="secondary-action" v-close-popup/>
          <q-btn unelevated label="开始迁移" color="primary" :disable="!plan" @click="startMigration"/>
        </q-card-actions>
      </q-card>
    </q-dialog>

    <LocalStorageMigrationDialog
      v-model="showProgress"
      :display="migrationDisplay"
      @retry="startMigration"
      @restart="relaunchApp"
    />
  </section>
</template>

<script setup lang="ts">
import {Channel} from '@tauri-apps/api/core';
import {open} from '@tauri-apps/plugin-dialog';
import {openPath} from '@tauri-apps/plugin-opener';
import {relaunch} from '@tauri-apps/plugin-process';
import {useQuasar} from 'quasar';
import {onMounted, ref} from 'vue';
import type {
  LocalStorageInfo,
  LocalStorageMigrationEvent,
  LocalStorageMigrationPlan,
} from '../../bindings';
import LocalStorageMigrationDialog from '../../components/LocalStorageMigrationDialog.vue';
import api from '../../utils/api';
import {formatBytes} from '../../utils/format';
import {formatError} from '../../utils/formatError';
import {
  initialLocalStorageMigrationDisplay,
  reduceLocalStorageMigrationDisplay,
  withLocalStorageMigrationError,
} from '../../utils/localStorageMigration';

const $q = useQuasar();
const info = ref<LocalStorageInfo>();
const plan = ref<LocalStorageMigrationPlan>();
const selectedBasePath = ref<string | null>(null);
const loadingInfo = ref(false);
const showPlan = ref(false);
const showProgress = ref(false);
const migrationDisplay = ref(initialLocalStorageMigrationDisplay());

onMounted(loadInfo);

async function loadInfo() {
  loadingInfo.value = true;
  try {
    info.value = await api.cmdGetLocalStorageInfo();
  } catch (error) {
    $q.notify({type: 'negative', message: `读取本地数据位置失败：${formatError(error)}`});
  } finally {
    loadingInfo.value = false;
  }
}

async function chooseLocation() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择本地数据存放目录',
  });
  if (typeof selected === 'string') {
    await planMigration(selected);
  }
}

async function openDataLocation() {
  if (!info.value) return;
  try {
    await openPath(info.value.currentPath);
  } catch (error) {
    $q.notify({type: 'negative', message: `打开本地数据位置失败：${formatError(error)}`});
  }
}

async function planMigration(basePath: string | null) {
  try {
    const nextPlan = await api.cmdPlanLocalStorageMigration(basePath);
    if (nextPlan.sourcePath === nextPlan.targetPath) {
      $q.notify({message: '本地数据已经位于所选位置'});
      return;
    }
    selectedBasePath.value = basePath;
    plan.value = nextPlan;
    showPlan.value = true;
  } catch (error) {
    $q.notify({type: 'negative', message: `无法迁移到所选位置：${formatError(error)}`});
  }
}

async function startMigration() {
  showPlan.value = false;
  showProgress.value = true;
  migrationDisplay.value = initialLocalStorageMigrationDisplay();
  const event = new Channel<LocalStorageMigrationEvent>();
  event.onmessage = (message) => {
    migrationDisplay.value = reduceLocalStorageMigrationDisplay(migrationDisplay.value, message);
  };

  try {
    await api.cmdMigrateLocalStorage(event, selectedBasePath.value);
  } catch (error) {
    if (!migrationDisplay.value.error) {
      migrationDisplay.value = withLocalStorageMigrationError(
        migrationDisplay.value,
        formatError(error),
      );
    }
  }
}

async function relaunchApp() {
  await relaunch();
}
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>

<style scoped lang="scss">
.storage-summary {
  display: flex;
  min-width: 0;
  max-width: 100%;
  gap: 5px;
}

.storage-path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.plan-dialog {
  width: min(520px, calc(100vw - 32px));
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-100);
  border-radius: var(--pad-radius-xl);
}

.dialog-title,
.detail-row strong,
.path-detail strong {
  color: var(--pad-text-color-200);
}

.dialog-description,
.detail-row span,
.path-detail span {
  color: var(--pad-text-color-400);
}

.plan-details {
  display: grid;
  gap: 10px;
}

.detail-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
}

.path-detail {
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr);
  gap: 8px;

  strong {
    overflow-wrap: anywhere;
    font-weight: 500;
  }
}

.secondary-action {
  color: var(--pad-text-color-300);
}

@media (max-width: 600px) {
  .path-separator,
  .storage-path {
    display: none;
  }
}
</style>
