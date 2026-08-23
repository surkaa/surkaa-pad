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

export const ANDROID_SHARE_RESUME_REFRESH_DELAYS_MS = [0, 150, 600] as const;

/**
 * Android 从来源应用返回时，原生 onNewIntent 与 WebView 恢复前台的先后顺序并不固定。
 * 立即读取一次并短暂重试，既覆盖生命周期时序，又避免在前台持续轮询。
 */
export function createAndroidShareResumeRefresher(
  refresh: () => void | Promise<void>,
  delays: readonly number[] = ANDROID_SHARE_RESUME_REFRESH_DELAYS_MS,
) {
  let timers: ReturnType<typeof setTimeout>[] = [];

  function cancelPending() {
    timers.forEach(timer => clearTimeout(timer));
    timers = [];
  }

  function trigger() {
    cancelPending();
    for (const delay of delays) {
      if (delay <= 0) {
        void refresh();
      } else {
        timers.push(setTimeout(() => void refresh(), delay));
      }
    }
  }

  return {
    trigger,
    dispose: cancelPending,
  };
}

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
