import type {AttachmentMeta} from "../bindings.ts";

export interface AttachmentTypeCounts {
  image: number;
  audio: number;
  video: number;
  file: number;
}

export function countAttachmentTypes(
    attachments: readonly Pick<AttachmentMeta, 'mimetype'>[],
): AttachmentTypeCounts {
  const counts: AttachmentTypeCounts = {image: 0, audio: 0, video: 0, file: 0};

  for (const attachment of attachments) {
    const mimetype = attachment.mimetype.toLowerCase();
    if (mimetype.startsWith('image/')) counts.image += 1;
    else if (mimetype.startsWith('audio/')) counts.audio += 1;
    else if (mimetype.startsWith('video/')) counts.video += 1;
    else counts.file += 1;
  }

  return counts;
}
