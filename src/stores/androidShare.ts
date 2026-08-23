import {computed, ref} from 'vue';
import {defineStore} from 'pinia';
import type {PendingAndroidShare} from '../bindings';
import api from '../utils/api';

const COMPLETED_BATCH_STORAGE_KEY = 'android-share-completed-batches';

function loadCompletedBatchIds(): Set<string> {
  try {
    const value = JSON.parse(localStorage.getItem(COMPLETED_BATCH_STORAGE_KEY) ?? '[]');
    return new Set(Array.isArray(value) ? value.filter(item => typeof item === 'string') : []);
  } catch {
    return new Set();
  }
}

export interface AndroidShareImportRequest {
  batchId: string;
  /** `null` 表示当前尚未持久化的新日记。 */
  targetDiaryId: string | null;
  phase: 'pending' | 'acknowledging';
}

export const useAndroidShareStore = defineStore('android-share', () => {
  const pendingBatches = ref<PendingAndroidShare[]>([]);
  const loading = ref(false);
  const importRequest = ref<AndroidShareImportRequest | null>(null);
  const selectingTargetBatchId = ref<string | null>(null);
  const completedBatchIds = ref(loadCompletedBatchIds());

  const pendingCount = computed(() => pendingBatches.value.length);
  const importingBatch = computed(() => importRequest.value
    ? pendingBatches.value.find(batch => batch.id === importRequest.value?.batchId) ?? null
    : null);
  const selectingTarget = computed(() => selectingTargetBatchId.value !== null);

  async function refresh() {
    if (loading.value) return;
    loading.value = true;
    try {
      const batches = await api.cmdListPendingAndroidShares();
      const visibleBatches: PendingAndroidShare[] = [];
      for (const batch of batches) {
        if (!completedBatchIds.value.has(batch.id)) {
          visibleBatches.push(batch);
          continue;
        }
        // 正文已经保存但上次确认被中断时，只重试确认，不再次展示和导入。
        try {
          await api.cmdAckPendingAndroidShare(batch.id);
          forgetCompletedBatch(batch.id);
        } catch (error) {
          console.error('重试确认已完成的 Android 分享失败:', error);
        }
      }
      pendingBatches.value = visibleBatches;
      if (
        importRequest.value
        && !pendingBatches.value.some(batch => batch.id === importRequest.value?.batchId)
      ) {
        importRequest.value = null;
      }
      if (
        selectingTargetBatchId.value
        && !pendingBatches.value.some(batch => batch.id === selectingTargetBatchId.value)
      ) {
        selectingTargetBatchId.value = null;
      }
    } finally {
      loading.value = false;
    }
  }

  function requestImport(batchId: string, targetDiaryId: string | null) {
    if (!pendingBatches.value.some(batch => batch.id === batchId)) {
      throw new Error('待导入的分享内容已经不存在');
    }
    selectingTargetBatchId.value = null;
    importRequest.value = {batchId, targetDiaryId, phase: 'pending'};
  }

  function clearImportRequest(batchId?: string) {
    if (!batchId || importRequest.value?.batchId === batchId) {
      importRequest.value = null;
    }
  }

  function markImportAwaitingAcknowledgement(batchId: string) {
    if (importRequest.value?.batchId !== batchId) {
      throw new Error('无法确认不存在的分享导入任务');
    }
    importRequest.value = {...importRequest.value, phase: 'acknowledging'};
    rememberCompletedBatch(batchId);
  }

  function persistCompletedBatchIds() {
    try {
      localStorage.setItem(
        COMPLETED_BATCH_STORAGE_KEY,
        JSON.stringify([...completedBatchIds.value].slice(-128)),
      );
    } catch (error) {
      console.error('保存 Android 分享完成标记失败:', error);
    }
  }

  function rememberCompletedBatch(batchId: string) {
    completedBatchIds.value.add(batchId);
    persistCompletedBatchIds();
  }

  function forgetCompletedBatch(batchId: string) {
    completedBatchIds.value.delete(batchId);
    persistCompletedBatchIds();
  }

  function beginTargetSelection(batchId: string) {
    if (!pendingBatches.value.some(batch => batch.id === batchId)) {
      throw new Error('待导入的分享内容已经不存在');
    }
    selectingTargetBatchId.value = batchId;
  }

  function cancelTargetSelection() {
    selectingTargetBatchId.value = null;
  }

  function selectTarget(targetDiaryId: string) {
    const batchId = selectingTargetBatchId.value;
    if (!batchId) throw new Error('当前没有正在选择目标的分享内容');
    requestImport(batchId, targetDiaryId);
    return batchId;
  }

  async function acknowledge(batchId: string) {
    await api.cmdAckPendingAndroidShare(batchId);
    forgetCompletedBatch(batchId);
    pendingBatches.value = pendingBatches.value.filter(batch => batch.id !== batchId);
    clearImportRequest(batchId);
    if (selectingTargetBatchId.value === batchId) selectingTargetBatchId.value = null;
  }

  return {
    pendingBatches,
    pendingCount,
    loading,
    importRequest,
    importingBatch,
    selectingTargetBatchId,
    selectingTarget,
    refresh,
    requestImport,
    clearImportRequest,
    markImportAwaitingAcknowledgement,
    beginTargetSelection,
    cancelTargetSelection,
    selectTarget,
    acknowledge,
  };
});
