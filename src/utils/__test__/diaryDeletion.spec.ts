import { describe, expect, it, vi } from 'vitest'
import { runDiaryDeletion } from '../diaryDeletion'

describe('runDiaryDeletion', () => {
  it('only updates the UI after storage deletion succeeds', async () => {
    const onDeleted = vi.fn()
    const onError = vi.fn()

    await expect(runDiaryDeletion(async () => {}, onDeleted, onError)).resolves.toBe(true)
    expect(onDeleted).toHaveBeenCalledOnce()
    expect(onError).not.toHaveBeenCalled()
  })

  it('keeps the diary visible when storage deletion fails', async () => {
    const error = new Error('partial delete')
    const onDeleted = vi.fn()
    const onError = vi.fn()

    await expect(runDiaryDeletion(async () => { throw error }, onDeleted, onError)).resolves.toBe(false)
    expect(onDeleted).not.toHaveBeenCalled()
    expect(onError).toHaveBeenCalledWith(error)
  })
})
