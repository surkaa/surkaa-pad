import { describe, expect, it } from 'vitest'
import { hasDiarySearchCriteria } from '../diarySearchFilters'

describe('hasDiarySearchCriteria', () => {
  it('rejects an empty search', () => {
    expect(hasDiarySearchCriteria('   ', [])).toBe(false)
  })

  it('allows keyword-only and attachment-only searches', () => {
    expect(hasDiarySearchCriteria('旅行', [])).toBe(true)
    expect(hasDiarySearchCriteria('', ['image'])).toBe(true)
  })
})
