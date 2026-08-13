export const GIB = 1024 ** 3;

export const ATTACHMENT_CACHE_LIMIT_OPTIONS = [1, 2, 5, 10, 20, 50, 100].map((gib) => ({
  label: `${gib} GB`,
  value: gib * GIB,
}));

export function attachmentCacheLimitLabel(limitBytes: number): string {
  const option = ATTACHMENT_CACHE_LIMIT_OPTIONS.find(({value}) => value === limitBytes);
  return option?.label ?? `${Math.round(limitBytes / GIB)} GB`;
}

