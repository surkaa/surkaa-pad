<script setup lang="ts">
import {addPluginListener, type PluginListener} from '@tauri-apps/api/core';
import {platform} from '@tauri-apps/plugin-os';
import {storeToRefs} from 'pinia';
import {computed, onBeforeUnmount, onMounted, ref, watch} from 'vue';
import {useQuasar} from 'quasar';
import {useRoute, useRouter} from 'vue-router';
import {useAndroidShareStore} from '../stores/androidShare';
import {useDataStore} from '../stores/data';
import {formatBytes} from '../utils/format';
import {formatError} from '../utils/formatError';

const isAndroid = platform() === 'android';
const $q = useQuasar();
const route = useRoute();
const router = useRouter();
const shareStore = useAndroidShareStore();
const dataStore = useDataStore();
const {pendingBatches, pendingCount, importRequest, selectingTarget} = storeToRefs(shareStore);
const {currentId, currentDiary} = storeToRefs(dataStore);
const dialogDismissed = ref(false);
let pluginListener: PluginListener | null = null;

const unlocked = computed(() => route.name !== 'Unlock');
const activeBatch = computed(() => pendingBatches.value[0] ?? null);
const showDialog = computed(() => Boolean(
  isAndroid
  && unlocked.value
  && activeBatch.value
  && !dialogDismissed.value
  && !selectingTarget.value
  && !importRequest.value,
));
const canUseCurrentDiary = computed(() => route.name === 'DiaryDetail');
const currentDiaryLabel = computed(() => currentId.value
  ? `添加到当前日记${currentDiary.value?.title ? `“${currentDiary.value.title}”` : ''}`
  : '添加到当前新日记');

function batchDescription() {
  const batch = activeBatch.value;
  if (!batch) return '';
  const parts: string[] = [];
  if (batch.text || batch.subject) parts.push('文字');
  if (batch.items.length) parts.push(`${batch.items.length} 个文件`);
  return parts.join('和');
}

async function refresh(showErrors = false) {
  if (!isAndroid) return;
  try {
    await shareStore.refresh();
  } catch (error) {
    console.error('读取 Android 分享内容失败:', error);
    if (showErrors) {
      $q.notify({type: 'negative', message: `读取分享内容失败：${formatError(error)}`});
    }
  }
}

async function routeToImport(batchId: string, targetDiaryId: string | null) {
  shareStore.requestImport(batchId, targetDiaryId);
  dataStore.currentId = targetDiaryId ?? '';
  if (route.name === 'DiaryDetail' && currentId.value === targetDiaryId) return;
  await router.push({
    name: 'DiaryDetail',
    query: {shareImport: batchId},
  });
}

async function importIntoCurrent() {
  const batch = activeBatch.value;
  if (!batch) return;
  dialogDismissed.value = true;
  await routeToImport(batch.id, currentId.value || null);
}

async function importIntoNewDiary() {
  const batch = activeBatch.value;
  if (!batch) return;
  dialogDismissed.value = true;
  shareStore.requestImport(batch.id, null);
  dataStore.currentId = '';
  await router.push({
    name: 'DiaryDetail',
    query: {shareImport: batch.id},
  });
}

async function chooseExistingDiary() {
  const batch = activeBatch.value;
  if (!batch) return;
  dialogDismissed.value = true;
  shareStore.beginTargetSelection(batch.id);
  await router.push({name: 'DiaryList'});
}

function discardBatch() {
  const batch = activeBatch.value;
  if (!batch) return;
  $q.dialog({
    title: '放弃导入',
    message: `确定放弃这批${batchDescription()}吗？需要时可以从来源应用重新分享。`,
    ok: {label: '放弃', color: 'negative', flat: true},
    cancel: {label: '取消', flat: true},
  }).onOk(async () => {
    try {
      await shareStore.acknowledge(batch.id);
      dialogDismissed.value = false;
    } catch (error) {
      $q.notify({type: 'negative', message: `放弃分享内容失败：${formatError(error)}`});
    }
  });
}

watch(() => route.name, name => {
  if (name !== 'DiaryList' && selectingTarget.value && !importRequest.value) {
    shareStore.cancelTargetSelection();
    dialogDismissed.value = false;
  }
  if (name !== 'Unlock') void refresh();
});

watch(pendingCount, (count, previous) => {
  if (count > previous) dialogDismissed.value = false;
});

watch(() => activeBatch.value?.id, (batchId, previousBatchId) => {
  if (batchId && previousBatchId && batchId !== previousBatchId && !importRequest.value) {
    dialogDismissed.value = false;
  }
});

onMounted(async () => {
  if (!isAndroid) return;
  await refresh(true);
  try {
    pluginListener = await addPluginListener<{pendingCount: number}>(
      'android-share-target',
      'pending-share',
      () => {
        dialogDismissed.value = false;
        void refresh(true);
      },
    );
  } catch (error) {
    console.error('监听 Android 分享事件失败:', error);
  }
});

onBeforeUnmount(() => {
  void pluginListener?.unregister();
  pluginListener = null;
});
</script>

<template>
  <div v-if="isAndroid && !unlocked && pendingCount" class="unlock-share-hint">
    <q-icon name="ios_share" size="20px"/>
    已收到 {{ pendingCount }} 批分享内容，解锁后即可导入
  </div>

  <q-btn
    v-if="isAndroid && unlocked && activeBatch && dialogDismissed && !selectingTarget && !importRequest"
    class="share-reopen-button"
    unelevated
    rounded
    color="primary"
    icon="ios_share"
    :label="`待导入 ${pendingCount} 批内容`"
    @click="dialogDismissed = false"
  />

  <q-dialog :model-value="showDialog" persistent no-refocus>
    <q-card class="share-import-dialog">
      <q-card-section class="row items-start no-wrap q-pb-sm">
        <q-avatar class="share-dialog-icon" icon="ios_share"/>
        <div class="q-ml-md">
          <div class="text-h6 dialog-title">导入分享内容</div>
          <div class="text-caption dialog-description">
            确认内容并选择要添加到的日记
          </div>
        </div>
      </q-card-section>

      <q-card-section v-if="activeBatch" class="share-preview q-pt-sm">
        <div v-if="activeBatch.subject" class="share-subject">{{ activeBatch.subject }}</div>
        <div v-if="activeBatch.text" class="share-text">{{ activeBatch.text }}</div>
        <q-list v-if="activeBatch.items.length" bordered separator class="share-file-list q-mt-md">
          <q-item v-for="item in activeBatch.items" :key="item.id" dense>
            <q-item-section avatar>
              <q-icon name="insert_drive_file" class="share-file-icon"/>
            </q-item-section>
            <q-item-section>
              <q-item-label class="ellipsis">{{ item.displayName }}</q-item-label>
              <q-item-label caption>
                {{ item.mimeType || '未知类型' }}
                <template v-if="item.size != null"> · {{ formatBytes(item.size) }}</template>
              </q-item-label>
            </q-item-section>
          </q-item>
        </q-list>
      </q-card-section>

      <q-card-section class="target-actions q-pt-sm">
        <q-btn
          v-if="canUseCurrentDiary"
          no-caps
          unelevated
          class="target-button"
          icon="edit_note"
          :label="currentDiaryLabel"
          @click="importIntoCurrent"
        />
        <q-btn
          v-if="!canUseCurrentDiary || currentId"
          no-caps
          unelevated
          class="target-button"
          icon="note_add"
          label="新建日记并导入"
          @click="importIntoNewDiary"
        />
        <q-btn
          no-caps
          unelevated
          class="target-button"
          icon="library_books"
          label="选择已有日记"
          @click="chooseExistingDiary"
        />
      </q-card-section>

      <q-card-actions align="between" class="q-px-md q-pb-md">
        <q-btn flat color="negative" label="放弃" @click="discardBatch"/>
        <q-btn flat label="稍后处理" @click="dialogDismissed = true"/>
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>

<style scoped lang="scss">
.unlock-share-hint,
.share-reopen-button {
  position: fixed;
  z-index: 7000;
  top: calc(env(safe-area-inset-top, 0px) + 12px);
  left: 50%;
  transform: translateX(-50%);
}

.unlock-share-hint {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: calc(100vw - 32px);
  padding: 10px 16px;
  border: 1px solid var(--pad-border-color-200);
  border-radius: var(--pad-radius-full);
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-200);
  box-shadow: var(--pad-shadow-md);
  font-size: 0.88rem;
}

.share-import-dialog {
  width: min(520px, calc(100vw - 28px));
  max-height: min(720px, calc(100vh - 48px));
  display: flex;
  flex-direction: column;
  border-radius: var(--pad-radius-xl);
  background: var(--pad-bg-color-100);
  color: var(--pad-text-color-100);
}

.share-dialog-icon {
  flex: 0 0 auto;
  background: var(--pad-bg-color-300);
  color: var(--pad-primary-color);
}

.dialog-title,
.share-subject {
  color: var(--pad-text-color-100);
}

.dialog-description,
.share-file-list :deep(.q-item__label--caption) {
  color: var(--pad-text-color-400);
}

.share-preview {
  min-height: 0;
  overflow-y: auto;
}

.share-subject {
  font-weight: 600;
  margin-bottom: 6px;
}

.share-text {
  max-height: 180px;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  color: var(--pad-text-color-200);
  line-height: 1.55;
}

.share-file-list {
  max-height: 230px;
  overflow-y: auto;
  border-color: var(--pad-border-color-100);
  border-radius: var(--pad-radius-lg);
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-200);
}

.share-file-icon {
  color: var(--pad-primary-color);
}

.target-actions {
  display: grid;
  gap: 8px;
}

.target-button {
  justify-content: flex-start;
  min-height: 46px;
  padding-inline: 14px;
  border: 1px solid var(--pad-border-color-100);
  border-radius: var(--pad-radius-lg);
  background: var(--pad-bg-color-200);
  color: var(--pad-text-color-100);
}
</style>
