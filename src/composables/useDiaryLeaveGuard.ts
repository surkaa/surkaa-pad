import {Dialog, useQuasar} from 'quasar';
import {nextTick, ref, type Ref} from 'vue';
import {onBeforeRouteLeave, type NavigationGuardNext} from 'vue-router';
import type {AttachmentMeta} from '../bindings';
import {useDataStore} from '../stores/data';
import api from '../utils/api';
import {getDiaryLeaveBlocker} from '../utils/diaryLeave';
import {formatError} from '../utils/formatError';

interface UseDiaryLeaveGuardOptions {
  diaryId: Ref<string>;
  unusedAttachments: Readonly<Ref<AttachmentMeta[]>>;
  isDeletingDiary: Readonly<Ref<boolean>>;
  hasActiveUploads: Readonly<Ref<boolean>>;
  showUploadDialog: Ref<boolean>;
  cancelAllUploads: () => Promise<boolean>;
  insertExistingAttachmentsAtEnd: (attachments: AttachmentMeta[]) => Promise<boolean>;
}

export function useDiaryLeaveGuard(options: UseDiaryLeaveGuardOptions) {
  const $q = useQuasar();
  const dataStore = useDataStore();
  const showUnusedAttachmentsDialog = ref(false);
  const unusedAttachmentActionLoading = ref(false);
  const pendingUnusedAttachments = ref<AttachmentMeta[]>([]);
  let pendingNavigation: NavigationGuardNext | null = null;

  function finishUnusedAttachmentCheck() {
    showUnusedAttachmentsDialog.value = false;
    pendingUnusedAttachments.value = [];
    const next = pendingNavigation;
    pendingNavigation = null;
    next?.();
  }

  async function appendUnusedAttachments() {
    unusedAttachmentActionLoading.value = true;
    try {
      const inserted = await options.insertExistingAttachmentsAtEnd(pendingUnusedAttachments.value);
      if (!inserted) throw new Error('编辑器未能插入附件');
      await nextTick();
      finishUnusedAttachmentCheck();
    } catch (error) {
      $q.notify({type: 'negative', message: `添加附件失败：${formatError(error)}`});
    } finally {
      unusedAttachmentActionLoading.value = false;
    }
  }

  async function deleteUnusedAttachments() {
    unusedAttachmentActionLoading.value = true;
    const attachments = [...pendingUnusedAttachments.value];
    try {
      await Promise.all(attachments.map(att => api.cmdDeleteAttachment(options.diaryId.value, att.id)));
      dataStore.deleteAttachment(options.diaryId.value, attachments.map(att => att.id));
      finishUnusedAttachmentCheck();
    } catch (error) {
      console.error('删除附件失败:', error);
      $q.notify({type: 'negative', message: `删除附件失败：${formatError(error)}`});
    } finally {
      unusedAttachmentActionLoading.value = false;
    }
  }

  function continueNavigation(next: NavigationGuardNext) {
    const blocker = getDiaryLeaveBlocker({
      isDeletingDiary: options.isDeletingDiary.value,
      hasActiveUploads: options.hasActiveUploads.value,
      unusedAttachmentCount: options.unusedAttachments.value.length,
    });

    if (blocker === 'active-uploads') {
      options.showUploadDialog.value = true;
      Dialog.create({
        title: '文件仍在处理中',
        message: '现在离开会取消尚未完成的任务。要取消任务并离开吗？',
        persistent: true,
        cancel: {flat: true, label: '继续等待'},
        ok: {unelevated: true, color: 'negative', label: '取消并离开'},
      })
        .onOk(async () => {
          const canceled = await options.cancelAllUploads();
          if (!canceled || options.hasActiveUploads.value) {
            $q.notify({type: 'negative', message: '仍有任务未能取消，请在任务弹窗中检查'});
            next(false);
            return;
          }
          continueNavigation(next);
        })
        .onCancel(() => next(false));
      return;
    }

    if (blocker === 'unused-attachments') {
      pendingUnusedAttachments.value = [...options.unusedAttachments.value];
      pendingNavigation = next;
      showUnusedAttachmentsDialog.value = true;
      return;
    }

    next();
  }

  onBeforeRouteLeave((_to, _from, next) => continueNavigation(next));

  return {
    showUnusedAttachmentsDialog,
    unusedAttachmentActionLoading,
    pendingUnusedAttachments,
    finishUnusedAttachmentCheck,
    appendUnusedAttachments,
    deleteUnusedAttachments,
  };
}
