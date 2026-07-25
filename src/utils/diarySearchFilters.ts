import type { AttachmentTypeFilter } from '../bindings'

export interface AttachmentTypeOption {
  label: string
  value: AttachmentTypeFilter
}

export const attachmentTypeOptions: AttachmentTypeOption[] = [
  { label: '图片', value: 'image' },
  { label: '录音', value: 'audio' },
  { label: '视频', value: 'video' },
  { label: '其他文件', value: 'other' },
]

export function hasDiarySearchCriteria(
  keyword: string,
  attachmentTypes: readonly AttachmentTypeFilter[],
): boolean {
  return keyword.trim().length > 0 || attachmentTypes.length > 0
}
