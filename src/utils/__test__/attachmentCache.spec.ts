import {describe, expect, it} from 'vitest';
import {
  ATTACHMENT_CACHE_LIMIT_OPTIONS,
  attachmentCacheLimitLabel,
  GIB,
} from '../attachmentCache';

describe('attachment cache settings', () => {
  it('provides bounded ascending cache limit options', () => {
    expect(ATTACHMENT_CACHE_LIMIT_OPTIONS.map(({value}) => value)).toEqual(
      [1, 2, 5, 10, 20, 50, 100].map((value) => value * GIB),
    );
  });

  it('formats configured and legacy cache limits', () => {
    expect(attachmentCacheLimitLabel(10 * GIB)).toBe('10 GB');
    expect(attachmentCacheLimitLabel(3 * GIB)).toBe('3 GB');
  });
});

