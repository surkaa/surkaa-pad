import {describe, expect, it} from 'vitest';
import {reorderPinnedDiaryIds} from '../pinnedDiaries';

describe('pinned diary ordering', () => {
  it.each([
    ['a', 2, ['b', 'c', 'a']], ['c', 0, ['c', 'a', 'b']],
    ['b', 2, ['a', 'c', 'b']], ['b', 0, ['b', 'a', 'c']],
    ['b', 1, ['a', 'b', 'c']], ['missing', 0, ['a', 'b', 'c']],
    ['b', -20, ['b', 'a', 'c']], ['b', 20, ['a', 'c', 'b']],
    ['b', 1.8, ['a', 'b', 'c']], ['b', NaN, ['a', 'b', 'c']],
    ['b', Infinity, ['a', 'b', 'c']],
  ] as const)('moves %s to insertion index %s without losing items', (id, index, expected) => {
    const original = Object.freeze(['a', 'b', 'c']);
    expect(reorderPinnedDiaryIds(original, id, index)).toEqual(expected);
    expect(original).toEqual(['a', 'b', 'c']);
  });
  it('handles empty and single-entry lists', () => {
    expect(reorderPinnedDiaryIds([], 'a', 0)).toEqual([]);
    expect(reorderPinnedDiaryIds(['a'], 'a', 20)).toEqual(['a']);
  });
});
