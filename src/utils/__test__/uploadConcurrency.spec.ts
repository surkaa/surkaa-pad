import {describe, expect, it} from 'vitest';
import {
  DEFAULT_UPLOAD_CONCURRENCY,
  normalizeUploadConcurrency,
} from '../uploadConcurrency';

describe('normalizeUploadConcurrency', () => {
  it('keeps integers inside the supported range', () => {
    expect(normalizeUploadConcurrency(1)).toBe(1);
    expect(normalizeUploadConcurrency(5)).toBe(5);
    expect(normalizeUploadConcurrency(20)).toBe(20);
  });

  it('clamps values to 1 through 20 and drops decimals', () => {
    expect(normalizeUploadConcurrency(0)).toBe(1);
    expect(normalizeUploadConcurrency(7.9)).toBe(7);
    expect(normalizeUploadConcurrency(21)).toBe(20);
  });

  it('falls back to the default for invalid values', () => {
    expect(normalizeUploadConcurrency(undefined)).toBe(DEFAULT_UPLOAD_CONCURRENCY);
    expect(normalizeUploadConcurrency('invalid')).toBe(DEFAULT_UPLOAD_CONCURRENCY);
  });
});
