import {AttachmentMeta} from "../bindings.ts";

export function resolveMediaAttachmentUrl(diaryId: string, attachment: AttachmentMeta): string {
    return `http://attachment.localhost/image/${diaryId}/${attachment.filename}`;
}