import {describe, expect, it} from 'vitest';
import {attachmentEncryptionFillPercentage} from '../attachmentEncryptionFill.ts';

describe('attachmentEncryptionFillPercentage', () => {
  it.each([
    [0, 10, 0],
    [5, 10, 50],
    [10, 10, 100],
    [1, 4, 25],
  ])('maps %d encrypted out of %d to %d%%', (encrypted, total, expected) => {
    expect(attachmentEncryptionFillPercentage(encrypted, total)).toBe(expected);
  });

  it('clamps inconsistent encrypted counts to the valid percentage range', () => {
    expect(attachmentEncryptionFillPercentage(11, 10)).toBe(100);
    expect(attachmentEncryptionFillPercentage(-1, 10)).toBe(0);
  });

  it('uses zero fill for empty or invalid counts', () => {
    expect(attachmentEncryptionFillPercentage(0, 0)).toBe(0);
    expect(attachmentEncryptionFillPercentage(1, Number.NaN)).toBe(0);
    expect(attachmentEncryptionFillPercentage(Number.POSITIVE_INFINITY, 1)).toBe(0);
  });
});
