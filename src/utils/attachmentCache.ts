export const MIB = 1024 ** 2;
export const GIB = 1024 ** 3;

export const ATTACHMENT_CACHE_LIMIT_OPTIONS = [1, 2, 5, 10, 20, 50, 100].map((gib) => ({
  label: `${gib} GB`,
  value: gib * GIB,
}));

export const ATTACHMENT_CACHE_FILE_SIZE_OPTIONS = [
  ...[50, 100, 200, 500].map((mib) => ({label: `${mib} MB`, value: mib * MIB})),
  ...[1, 2, 5, 10, 20, 50, 100].map((gib) => ({label: `${gib} GB`, value: gib * GIB})),
];

export function attachmentCacheLimitLabel(limitBytes: number): string {
  const option = ATTACHMENT_CACHE_LIMIT_OPTIONS.find(({value}) => value === limitBytes);
  return option?.label ?? `${Math.round(limitBytes / GIB)} GB`;
}

export function attachmentCacheFileSizeLabel(limitBytes: number): string {
  const option = ATTACHMENT_CACHE_FILE_SIZE_OPTIONS.find(({value}) => value === limitBytes);
  if (option) return option.label;
  return limitBytes >= GIB
    ? `${Math.round(limitBytes / GIB)} GB`
    : `${Math.round(limitBytes / MIB)} MB`;
}

export function partitionAttachmentsByCacheLimit(
  attachmentIds: string[],
  attachments: Array<{id: string; size: number}>,
  maxFileSizeBytes: number,
): {cacheableIds: string[]; oversizedIds: string[]} {
  const attachmentById = new Map(attachments.map((attachment) => [attachment.id, attachment]));
  const oversizedIds: string[] = [];
  const cacheableIds = attachmentIds.filter((id) => {
    const attachment = attachmentById.get(id);
    if (attachment && attachment.size > maxFileSizeBytes) {
      oversizedIds.push(id);
      return false;
    }
    return true;
  });
  return {cacheableIds, oversizedIds};
}
