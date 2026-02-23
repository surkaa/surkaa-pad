export function resolveMediaAttachmentUrl(type: 'image' | 'audio' | 'video', diaryId: string, filename: string): string {
    return `http://attachment.localhost/${type}/${diaryId}/${filename}`;
}
