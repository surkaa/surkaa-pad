import { Extension } from '@tiptap/vue-3'
import type { AttachmentMeta } from '../../../bindings'

export interface AttachmentStorageOptions {
  attachmentMap: Record<string, string>
  getAttachment: (filename: string) => AttachmentMeta | null
}

export const AttachmentStorage = Extension.create<AttachmentStorageOptions>({
  name: 'attachmentStorage',

  addOptions() {
    return {
      attachmentMap: {},
      getAttachment: () => null,
    }
  },

  addStorage() {
    return {
      attachmentMap: {} as Record<string, string>,
      getAttachment: null as ((filename: string) => AttachmentMeta | null) | null,
    }
  },

  onCreate() {
    this.storage.attachmentMap = this.options.attachmentMap
    this.storage.getAttachment = this.options.getAttachment
  },
})
