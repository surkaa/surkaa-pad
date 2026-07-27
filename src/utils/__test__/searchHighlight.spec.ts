import {describe, expect, it} from 'vitest'
import {findSearchHighlightRanges, normalizeSearchTerms} from '../searchHighlight'

describe('normalizeSearchTerms', () => {
  it('splits on whitespace and removes duplicates', () => {
    expect(normalizeSearchTerms('  日记\n测试  日记\t图片 ')).toEqual(['日记', '测试', '图片'])
  })

  it('returns no terms for whitespace-only input', () => {
    expect(normalizeSearchTerms(' \n\t ')).toEqual([])
  })
})

describe('findSearchHighlightRanges', () => {
  it('finds every exact case-sensitive occurrence', () => {
    expect(findSearchHighlightRanges('test Test test', ['test'])).toEqual([
      {from: 0, to: 4},
      {from: 10, to: 14},
    ])
  })

  it('sorts and merges overlapping term matches', () => {
    expect(findSearchHighlightRanges('abcab', ['bc', 'abc', 'ab'])).toEqual([
      {from: 0, to: 3},
      {from: 3, to: 5},
    ])
  })
})
