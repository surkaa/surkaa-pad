export function attachmentEncryptionFillPercentage(
    encryptedCount: number,
    totalCount: number,
): number {
  if (!Number.isFinite(encryptedCount)
      || !Number.isFinite(totalCount)
      || totalCount <= 0) return 0;

  return Math.min(100, Math.max(0, encryptedCount / totalCount * 100));
}
