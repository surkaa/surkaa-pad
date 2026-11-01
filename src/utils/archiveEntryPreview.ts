import type {PreparedArchiveEntry, ArchiveEntryProgress} from '../bindings';
import {attachmentPreviewKind} from './attachmentPreview';
import {isHtmlAttachment} from './attachmentOpen';

export function archiveEntryKind(resource: Pick<PreparedArchiveEntry, 'filename' | 'mimetype'>) {
  if (isHtmlAttachment(resource)) return 'html' as const;
  const mime = resource.mimetype.toLowerCase();
  const segments = resource.filename.split('.');
  const extension = segments[segments.length - 1]?.toLowerCase() || '';
  if (['zip', '7z', 'rar', 'tar', 'gz', 'bz2', 'xz', 'tgz', 'zst'].includes(extension)) return null;
  if (mime.startsWith('image/') || ['jpg', 'jpeg', 'png', 'gif', 'webp', 'avif', 'heic', 'heif', 'bmp', 'svg', 'ico'].includes(extension)) return 'image' as const;
  if (mime.startsWith('audio/') || ['mp3', 'wav', 'm4a', 'flac', 'aac', 'ogg', 'opus', 'amr'].includes(extension)) return 'audio' as const;
  if (mime.startsWith('video/') || ['mp4', 'mov', 'm4v', 'webm', 'mkv', 'avi', '3gp'].includes(extension)) return 'video' as const;
  const kind = attachmentPreviewKind(resource);
  return kind === 'archive' ? null : kind;
}

export function archiveEntryProgressValue(progress: ArchiveEntryProgress | null): number {
  // 准备阶段不展示虚拟进度，校验阶段也不再退回等待动画。
  if (progress?.phase === 'verifying') return 1;
  if (!progress || progress.phase !== 'extracting' || !Number.isFinite(progress.totalBytes) || !Number.isFinite(progress.processedBytes) || progress.totalBytes <= 0) return 0;
  return Math.max(0, Math.min(1, progress.processedBytes / progress.totalBytes));
}
