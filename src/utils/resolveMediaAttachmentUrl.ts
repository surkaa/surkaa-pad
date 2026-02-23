export const SUPPORT_TYPES = ['image', 'audio', 'video'] as const;

export type SupportType = typeof SUPPORT_TYPES[number];

export function resolveMediaAttachmentUrl(type: SupportType, diaryId: string, filename: string): string {
    return `http://attachment.localhost/${type}/${diaryId}/${filename}`;
}
