// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

import { useDataStore } from '../data'
import type {AttachmentMeta, DiarySummary} from '../../bindings'

function attachment(id = 'att-1'): AttachmentMeta {
  return {
    id,
    filename: `${id}.txt`,
    mimetype: 'text/plain',
    size: 1,
    encrypted: false,
    nonce: [],
    algorithm: 'AES256-GCM_v1',
  }
}

function summary(id = 123): DiarySummary {
  return {
    id,
    created: 1,
    updated: 1,
    title: 'title',
    attachmentCount: 0,
    attachmentTotalSize: 0,
    attachmentCounts: {image: 0, audio: 0, video: 0, file: 0},
    encryptedAttachmentCounts: {image: 0, audio: 0, video: 0, file: 0},
  }
}

describe('data store diary list invalidation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
  })

  it('clears stale list state and advances its revision', () => {
    const store = useDataStore()
    store.diaryIds.push(123)
    store.diarySummaries[123] = null
    store.currentId = 123
    store.currentDiaryAttachments = [attachment()]
    store.currentDiaryAttachmentUrlMap = {'att-1': 'local-url'}
    const revision = store.diaryListRevision

    store.invalidateDiaryList()

    expect(store.diaryIds).toEqual([])
    expect(store.diarySummaries).toEqual({})
    expect(store.currentId).toBe(0)
    expect(store.currentDiaryAttachments).toEqual([])
    expect(store.currentDiaryAttachmentUrlMap).toEqual({})
    expect(store.diaryListRevision).toBe(revision + 1)
  })

  it('updates only the opened diary attachment details and keeps the summary count lightweight', () => {
    const store = useDataStore()
    store.currentId = 123
    store.diarySummaries[123] = summary()

    store.updateAttachment(123, attachment())
    store.updateAttachment(123, {...attachment(), size: 2})
    store.updateAttachmentFilename(123, 'att-1', 'renamed.txt')

    expect(store.currentDiaryAttachments).toHaveLength(1)
    expect(store.currentDiaryAttachments[0]).toMatchObject({filename: 'renamed.txt', size: 2})
    expect(store.diarySummaries[123]?.attachmentCount).toBe(1)

    store.deleteAttachment(123, ['att-1'])

    expect(store.currentDiaryAttachments).toEqual([])
    expect(store.diarySummaries[123]?.attachmentCount).toBe(0)
  })
})
