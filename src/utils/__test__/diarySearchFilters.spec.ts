import { describe, expect, it } from 'vitest'
import {
  hasDiarySearchCriteria,
  normalizeAttachmentFilterSelection,
  selectedAttachmentTypes,
} from '../diarySearchFilters'

describe('hasDiarySearchCriteria', () => {
  it('rejects an empty search', () => {
    expect(hasDiarySearchCriteria('   ', [])).toBe(false)
  })

  it('allows keyword-only and attachment-only searches', () => {
    expect(hasDiarySearchCriteria('旅行', [])).toBe(true)
    expect(hasDiarySearchCriteria('', ['image'])).toBe(true)
  })
})

describe('normalizeAttachmentFilterSelection', () => {
  it('uses no-filter as the stable default', () => {
    expect(normalizeAttachmentFilterSelection([], [])).toEqual(['all'])
    expect(normalizeAttachmentFilterSelection(['all'], [])).toEqual(['all'])
  })

  it('replaces no-filter when a concrete type is selected', () => {
    expect(normalizeAttachmentFilterSelection(['all'], ['all', 'image']))
      .toEqual(['image'])
  })

  it('allows multiple concrete attachment types', () => {
    expect(normalizeAttachmentFilterSelection(['image'], ['image', 'audio']))
      .toEqual(['image', 'audio'])
  })

  it('clears concrete types when no-filter is selected', () => {
    expect(normalizeAttachmentFilterSelection(['image', 'audio'], ['image', 'audio', 'all']))
      .toEqual(['all'])
  })

  it('restores no-filter after the last concrete type is removed', () => {
    expect(normalizeAttachmentFilterSelection(['video'], [])).toEqual(['all'])
  })
})

describe('selectedAttachmentTypes', () => {
  it('maps no-filter to the empty backend filter and preserves concrete types', () => {
    expect(selectedAttachmentTypes(['all'])).toEqual([])
    expect(selectedAttachmentTypes(['image', 'other'])).toEqual(['image', 'other'])
  })
})
