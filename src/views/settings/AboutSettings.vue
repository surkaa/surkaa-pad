<template>
  <section class="settings-group">
    <div class="group-title">关于</div>
    <q-list bordered class="pad-card">
      <q-item clickable v-ripple class="settings-item" @click="showDialog = true">
        <q-item-section avatar class="settings-icon-section">
          <q-icon name="info"/>
        </q-item-section>
        <q-item-section>
          <q-item-label class="label-text text-weight-medium">SurKaa Pad</q-item-label>
          <q-item-label caption class="desc-text">
            {{ loading ? '正在读取版本信息…' : summary }}
          </q-item-label>
        </q-item-section>
        <q-item-section side>
          <q-icon name="chevron_right" class="desc-text"/>
        </q-item-section>
      </q-item>
    </q-list>

    <q-dialog v-model="showDialog">
      <q-card class="about-dialog">
        <q-card-section class="about-heading">
          <img src="/app-icon.png" alt="SurKaa Pad Logo" class="about-logo"/>
          <div>
            <div class="text-h6 title-text">{{ info?.appName || 'SurKaa Pad' }}</div>
            <div class="text-caption desc-text">本地优先、端到端加密的个人日记</div>
          </div>
        </q-card-section>

        <q-card-section v-if="loading" class="about-loading">
          <q-spinner color="primary" size="28px"/>
          <span>正在读取应用信息…</span>
        </q-card-section>
        <q-card-section v-else-if="error" class="about-error">
          <q-icon name="error_outline"/>
          <span>读取应用信息失败：{{ error }}</span>
        </q-card-section>
        <q-list v-else-if="info" separator class="about-details">
          <q-item v-for="item in detailItems" :key="item.label" dense>
            <q-item-section class="detail-label">{{ item.label }}</q-item-section>
            <q-item-section side class="detail-value">{{ item.value }}</q-item-section>
          </q-item>
        </q-list>

        <q-card-actions align="right">
          <q-btn flat label="关闭" color="primary" v-close-popup/>
        </q-card-actions>
      </q-card>
    </q-dialog>
  </section>
</template>

<script setup lang="ts">
import {computed, onMounted, ref} from 'vue';
import {formatAboutSummary, loadAboutInfo, type AboutInfo} from '../../utils/aboutInfo';
import {formatError} from '../../utils/formatError';

const info = ref<AboutInfo | null>(null);
const loading = ref(true);
const error = ref<string | null>(null);
const showDialog = ref(false);

const summary = computed(() => {
  if (info.value) return formatAboutSummary(info.value);
  return error.value ? '版本信息读取失败' : '版本信息不可用';
});
const detailItems = computed(() => info.value ? [
  {label: '应用版本', value: info.value.appVersion},
  {label: 'Git 提交', value: info.value.gitCommit},
  {label: '应用标识', value: info.value.identifier},
  {label: 'Tauri 版本', value: info.value.tauriVersion},
  {label: '运行平台', value: `${info.value.platform} ${info.value.architecture}`},
  {label: '系统版本', value: info.value.osVersion},
] : []);

onMounted(refreshInfo);

async function refreshInfo() {
  loading.value = true;
  error.value = null;
  try {
    info.value = await loadAboutInfo();
  } catch (loadError) {
    info.value = null;
    error.value = formatError(loadError);
  } finally {
    loading.value = false;
  }
}
</script>

<style scoped lang="scss" src="./settingsSection.scss"></style>
<style scoped lang="scss">
.about-dialog {
  width: min(420px, calc(100vw - 32px));
  color: var(--pad-text-color-100);
  background: var(--pad-bg-color-100);
  border-radius: var(--pad-radius-xl);
}

.about-heading {
  display: flex;
  align-items: center;
  gap: 12px;
}

.about-logo {
  width: 44px;
  height: 44px;
  flex: 0 0 auto;
  object-fit: contain;
  border-radius: 10px;
}

.title-text {
  color: var(--pad-text-color-100);
}

.desc-text,
.detail-label,
.about-loading {
  color: var(--pad-text-color-400);
}

.about-loading,
.about-error {
  display: flex;
  align-items: center;
  gap: 10px;
}

.about-error {
  color: var(--pad-danger-color);
}

.about-details {
  margin: 0 16px;
  overflow: hidden;
  color: var(--pad-text-color-200);
  background: var(--pad-bg-color-200);
  border: 1px solid var(--pad-border-color-100);
  border-radius: var(--pad-radius-md);
}

.detail-value {
  max-width: 68%;
  padding-left: 16px;
  color: var(--pad-text-color-200);
  overflow-wrap: anywhere;
  text-align: right;
}
</style>
