export const MIN_UPLOAD_CONCURRENCY = 1;
export const DEFAULT_UPLOAD_CONCURRENCY = 5;
export const MAX_UPLOAD_CONCURRENCY = 20;

export function normalizeUploadConcurrency(value: unknown): number {
  const parsed = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(parsed)) return DEFAULT_UPLOAD_CONCURRENCY;
  return Math.min(
    MAX_UPLOAD_CONCURRENCY,
    Math.max(MIN_UPLOAD_CONCURRENCY, Math.trunc(parsed)),
  );
}
