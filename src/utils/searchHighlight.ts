export interface TextRange {
  from: number
  to: number
}

export function normalizeSearchTerms(keyword: string): string[] {
  return [...new Set(keyword.split(/\s+/u).filter(Boolean))]
}

export function findSearchHighlightRanges(text: string, terms: readonly string[]): TextRange[] {
  const ranges: TextRange[] = []
  for (const term of terms) {
    if (!term) continue
    let from = 0
    while (from < text.length) {
      const index = text.indexOf(term, from)
      if (index < 0) break
      ranges.push({from: index, to: index + term.length})
      from = index + term.length
    }
  }

  ranges.sort((left, right) => left.from - right.from || left.to - right.to)
  return ranges.reduce<TextRange[]>((merged, range) => {
    const previous = merged[merged.length - 1]
    if (previous && range.from < previous.to) {
      previous.to = Math.max(previous.to, range.to)
    } else {
      merged.push({...range})
    }
    return merged
  }, [])
}
