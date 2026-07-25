import type { AttachmentTypeFilter } from '../bindings'

export const NO_ATTACHMENT_FILTER = 'all' as const

export type AttachmentFilterSelection = AttachmentTypeFilter | typeof NO_ATTACHMENT_FILTER

export interface AttachmentTypeOption {
  label: string
  value: AttachmentFilterSelection
}

export const attachmentTypeOptions: AttachmentTypeOption[] = [
  { label: '不限', value: NO_ATTACHMENT_FILTER },
  { label: '图片', value: 'image' },
  { label: '录音', value: 'audio' },
  { label: '视频', value: 'video' },
  { label: '文件', value: 'other' },
]

export function normalizeAttachmentFilterSelection(
  previous: readonly AttachmentFilterSelection[],
  next: readonly AttachmentFilterSelection[],
): AttachmentFilterSelection[] {
  const noFilterWasAdded = next.includes(NO_ATTACHMENT_FILTER)
    && !previous.includes(NO_ATTACHMENT_FILTER)
  if (noFilterWasAdded) {
    return [NO_ATTACHMENT_FILTER]
  }

  const attachmentTypes = next.filter(
    (value): value is AttachmentTypeFilter => value !== NO_ATTACHMENT_FILTER,
  )
  return attachmentTypes.length > 0 ? attachmentTypes : [NO_ATTACHMENT_FILTER]
}

export function toggleAttachmentFilterSelection(
  previous: readonly AttachmentFilterSelection[],
  value: AttachmentFilterSelection,
): AttachmentFilterSelection[] {
  const next = previous.includes(value)
    ? previous.filter(selected => selected !== value)
    : [...previous, value]
  return normalizeAttachmentFilterSelection(previous, next)
}

export function selectedAttachmentTypes(
  selection: readonly AttachmentFilterSelection[],
): AttachmentTypeFilter[] {
  return selection.filter(
    (value): value is AttachmentTypeFilter => value !== NO_ATTACHMENT_FILTER,
  )
}

export function hasDiarySearchCriteria(
  keyword: string,
  attachmentTypes: readonly AttachmentTypeFilter[],
): boolean {
  return keyword.trim().length > 0 || attachmentTypes.length > 0
}
