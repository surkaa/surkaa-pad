import {computed, ref} from 'vue';
import {defineStore} from 'pinia';
import type {PendingAndroidShare} from '../bindings';
import api from '../utils/api';

export interface AndroidShareImportRequest {
  batchId: string;
  /** `null` 表示当前尚未持久化的新日记。 */
  targetDiaryId: string | null;
}

export const useAndroidShareStore = defineStore('android-share', () => {
  const pendingBatches = ref<PendingAndroidShare[]>([]);
  const loading = ref(false);
  const importRequest = ref<AndroidShareImportRequest | null>(null);
  const selectingTargetBatchId = ref<string | null>(null);

  const pendingCount = computed(() => pendingBatches.value.length);
  const importingBatch = computed(() => importRequest.value
    ? pendingBatches.value.find(batch => batch.id === importRequest.value?.batchId) ?? null
    : null);
  const selectingTarget = computed(() => selectingTargetBatchId.value !== null);

  async function refresh() {
    if (loading.value) return;
    loading.value = true;
    try {
      pendingBatches.value = await api.cmdListPendingAndroidShares();
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
    importRequest.value = {batchId, targetDiaryId};
  }

  function clearImportRequest(batchId?: string) {
    if (!batchId || importRequest.value?.batchId === batchId) {
      importRequest.value = null;
    }
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
    beginTargetSelection,
    cancelTargetSelection,
    selectTarget,
    acknowledge,
  };
});
