// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/plugin-store', () => ({
  Store: {
    load: vi.fn().mockResolvedValue({
      length: vi.fn().mockResolvedValue(0),
      get: vi.fn().mockResolvedValue(null),
    }),
  },
}))

import { useDataStore } from '../data'

describe('data store diary list invalidation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    localStorage.setItem('config:migrated', 'true')
  })

  it('clears stale list state and advances its revision', () => {
    const store = useDataStore()
    store.diaryIds.push('123')
    store.diarySummaries['123'] = null
    store.currentId = '123'
    store.currentDiaryAttachmentUrlMap = {'att-1': 'local-url'}
    const revision = store.diaryListRevision

    store.invalidateDiaryList()

    expect(store.diaryIds).toEqual([])
    expect(store.diarySummaries).toEqual({})
    expect(store.currentId).toBe('')
    expect(store.currentDiaryAttachmentUrlMap).toEqual({})
    expect(store.diaryListRevision).toBe(revision + 1)
  })
})
