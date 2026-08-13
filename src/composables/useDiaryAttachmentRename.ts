import type { Ref } from 'vue'
import { ref } from 'vue'
import { useQuasar } from 'quasar'
import { useDataStore } from '../stores/data'
import api from '../utils/api'
import { formatError } from '../utils/formatError'

type AttachmentRenamedCallback = (newFilename: string) => void

export function useDiaryAttachmentRename(diaryId: Ref<number>) {
  const $q = useQuasar()
  const dataStore = useDataStore()
  const showRenameDialog = ref(false)
  const attachmentId = ref('')
  const oldFilename = ref('')
  const newFilename = ref('')
  let renamedCallback: AttachmentRenamedCallback | null = null

  function requestRename(
    id: string,
    filename: string,
    callback: AttachmentRenamedCallback,
  ) {
    attachmentId.value = id
    oldFilename.value = filename
    newFilename.value = filename
    renamedCallback = callback
    showRenameDialog.value = true
  }

  function closeRenameDialog() {
    showRenameDialog.value = false
    attachmentId.value = ''
    oldFilename.value = ''
    newFilename.value = ''
    renamedCallback = null
  }

  async function confirmRename() {
    const filename = newFilename.value.trim()
    if (!filename || filename === oldFilename.value) {
      closeRenameDialog()
      return
    }

    try {
      await api.cmdUpdateAttachmentFilename(diaryId.value, attachmentId.value, filename)
      dataStore.updateAttachmentFilename(diaryId.value, attachmentId.value, filename)
      renamedCallback?.(filename)
      closeRenameDialog()
    } catch (error) {
      $q.notify({ type: 'negative', message: formatError(error) })
    }
  }

  return {
    showRenameDialog,
    oldFilename,
    newFilename,
    requestRename,
    closeRenameDialog,
    confirmRename,
  }
}
