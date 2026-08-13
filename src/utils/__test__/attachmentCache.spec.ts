import {describe, expect, it} from 'vitest';
import {
  ATTACHMENT_CACHE_LIMIT_OPTIONS,
  ATTACHMENT_CACHE_FILE_SIZE_OPTIONS,
  attachmentCacheFileSizeLabel,
  attachmentCacheLimitLabel,
  GIB,
  MIB,
  partitionAttachmentsByCacheLimit,
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

  it('provides a 100 MB default-compatible per-file limit option', () => {
    expect(ATTACHMENT_CACHE_FILE_SIZE_OPTIONS).toContainEqual({label: '100 MB', value: 100 * MIB});
    expect(attachmentCacheFileSizeLabel(100 * MIB)).toBe('100 MB');
    expect(attachmentCacheFileSizeLabel(3 * GIB)).toBe('3 GB');
  });

  it('separates oversized attachments while retaining unknown ids for backend validation', () => {
    const attachments = [
      {id: 'small', size: 50 * MIB},
      {id: 'large', size: 101 * MIB},
    ];

    expect(partitionAttachmentsByCacheLimit(
      ['small', 'large', 'missing'],
      attachments,
      100 * MIB,
    )).toEqual({
      cacheableIds: ['small', 'missing'],
      oversizedIds: ['large'],
    });
  });
});
