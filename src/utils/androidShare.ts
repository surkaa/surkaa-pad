import type {
  DiaryContent,
  DiaryContentNode,
  PendingAndroidShare,
} from '../bindings';
import {
  attachmentNodeKindFromMimeType,
  planAttachmentInsertions,
  type AttachmentNodeKind,
  type AttachmentInsertion,
  type UploadedAttachment,
} from './attachmentInsertion';

/**
 * Android 来源声明的音视频类型优先，避免例如 M4A 被内容探测为 video/mp4；
 * 来源未提供具体媒体类型时才退回后端探测结果。
 */
export function sharedAttachmentNodeKind(
  providerMimetype: string | null | undefined,
  detectedMimetype: string,
): AttachmentNodeKind {
  const providerKind = attachmentNodeKindFromMimeType(providerMimetype ?? '');
  return providerKind === 'file'
    ? attachmentNodeKindFromMimeType(detectedMimetype)
    : providerKind;
}

export function androidShareText(
  subject: string | null | undefined,
  text: string | null | undefined,
): string {
  const normalizedSubject = subject?.trim() ?? '';
  const normalizedText = text?.trim() ?? '';
  if (!normalizedSubject) return normalizedText;
  if (!normalizedText) return normalizedSubject;
  if (normalizedText.startsWith(normalizedSubject)) return normalizedText;
  return `${normalizedSubject}\n\n${normalizedText}`;
}

export function attachmentInsertionsToDiaryNodes(
  insertions: readonly AttachmentInsertion[],
): DiaryContentNode[] {
  return insertions.map(insertion => {
    switch (insertion.type) {
      case 'image':
        return {type: 'image', attachmentId: insertion.attachmentId, size: 'normal'};
      case 'audio':
        return {type: 'audio', attachmentId: insertion.attachmentId};
      case 'video':
        return {type: 'video', attachmentId: insertion.attachmentId};
      case 'file':
        return {type: 'file', attachmentId: insertion.attachmentId};
      case 'album':
        return {
          type: 'album',
          id: insertion.id,
          attachmentIds: insertion.images,
          displayMode: 'horizontalList',
        };
    }
  });
}

/** 在现有正文末尾追加分享文字和附件，原内容及分享顺序保持不变。 */
export function appendAndroidShareToDiaryContent(
  content: DiaryContent,
  batch: PendingAndroidShare,
  uploadedAttachments: readonly UploadedAttachment[],
  createAlbumId: () => string,
): DiaryContent {
  const nodes = [...content.nodes];
  const text = androidShareText(batch.subject, batch.text);
  if (text) nodes.push({type: 'markdown', text});
  nodes.push(...attachmentInsertionsToDiaryNodes(
    planAttachmentInsertions(uploadedAttachments, createAlbumId),
  ));
  return {nodes};
}
