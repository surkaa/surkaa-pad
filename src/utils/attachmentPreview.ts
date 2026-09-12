import type {AttachmentMeta} from '../bindings';
import {isHtmlAttachment} from './attachmentOpen';

export const MAX_TEXT_PREVIEW_BYTES = 5 * 1024 * 1024;

export type AttachmentPreviewKind = 'pdf' | 'markdown' | 'json' | 'text' | 'archive';

const TEXT_FILE_EXTENSIONS = new Set([
  'txt', 'log', 'csv', 'tsv', 'xml', 'yaml', 'yml', 'toml', 'ini', 'conf', 'cfg',
  'properties', 'sql', 'rs', 'js', 'jsx', 'ts', 'tsx', 'vue', 'css', 'scss', 'sass',
  'less', 'py', 'java', 'kt', 'kts', 'c', 'cc', 'cpp', 'h', 'hpp', 'go', 'sh',
  'ps1', 'bat', 'cmd',
]);

export class AttachmentPreviewError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'AttachmentPreviewError';
  }
}

export function attachmentPreviewKind(
  attachment: Pick<AttachmentMeta, 'filename' | 'mimetype'>,
): AttachmentPreviewKind | null {
  if (isHtmlAttachment(attachment)) return null;

  const mediaType = normalizedMediaType(attachment.mimetype);
  const extension = filenameExtension(attachment.filename);
  if (
    mediaType === 'application/zip'
    || mediaType === 'application/x-zip-compressed'
    || mediaType === 'application/x-7z-compressed'
    || extension === 'zip'
    || extension === '7z'
  ) return 'archive';
  if (mediaType === 'application/pdf' || extension === 'pdf') return 'pdf';
  if (
    mediaType === 'text/markdown'
    || mediaType === 'text/x-markdown'
    || extension === 'md'
    || extension === 'markdown'
  ) return 'markdown';
  if (
    mediaType === 'application/json'
    || mediaType === 'text/json'
    || mediaType.endsWith('+json')
    || extension === 'json'
  ) return 'json';
  if (
    mediaType.startsWith('text/')
    || mediaType === 'application/xml'
    || mediaType.endsWith('+xml')
    || TEXT_FILE_EXTENSIONS.has(extension)
  ) return 'text';
  return null;
}

export function buildPdfPreviewUrl(attachmentUrl: string): string {
  const url = new URL(attachmentUrl);
  url.searchParams.set('view', 'pdf');
  return url.toString();
}

export async function fetchAttachmentText(
  url: string,
  declaredSize: number,
  signal?: AbortSignal,
  maxBytes = MAX_TEXT_PREVIEW_BYTES,
): Promise<string> {
  if (!Number.isSafeInteger(declaredSize) || declaredSize < 0) {
    throw new AttachmentPreviewError('附件大小无效，无法安全预览');
  }
  if (declaredSize > maxBytes) throw previewTooLargeError(maxBytes);

  const response = await fetch(url, {signal, cache: 'no-store'});
  if (!response.ok) {
    throw new AttachmentPreviewError(`读取附件失败（HTTP ${response.status}）`);
  }
  return decodeText(await readBoundedResponseBytes(response, maxBytes));
}

export async function readBoundedResponseBytes(
  response: Response,
  maxBytes = MAX_TEXT_PREVIEW_BYTES,
): Promise<Uint8Array> {
  const contentLength = response.headers.get('content-length');
  if (contentLength !== null) {
    const parsedLength = Number(contentLength);
    if (Number.isFinite(parsedLength) && parsedLength > maxBytes) {
      await response.body?.cancel();
      throw previewTooLargeError(maxBytes);
    }
  }

  if (!response.body) {
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.byteLength > maxBytes) throw previewTooLargeError(maxBytes);
    return bytes;
  }

  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const {done, value} = await reader.read();
      if (done) break;
      if (!value?.byteLength) continue;
      total += value.byteLength;
      if (total > maxBytes) {
        await reader.cancel();
        throw previewTooLargeError(maxBytes);
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }

  const result = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return result;
}

export function decodeText(bytes: Uint8Array): string {
  if (hasPrefix(bytes, [0xef, 0xbb, 0xbf])) {
    return new TextDecoder('utf-8').decode(bytes.subarray(3));
  }
  if (hasPrefix(bytes, [0xff, 0xfe])) {
    return new TextDecoder('utf-16le').decode(bytes.subarray(2));
  }
  if (hasPrefix(bytes, [0xfe, 0xff])) {
    return new TextDecoder('utf-16be').decode(bytes.subarray(2));
  }

  try {
    return new TextDecoder('utf-8', {fatal: true}).decode(bytes);
  } catch {
    try {
      return new TextDecoder('gb18030', {fatal: true}).decode(bytes);
    } catch {
      return new TextDecoder('utf-8').decode(bytes);
    }
  }
}

function normalizedMediaType(mimetype: string): string {
  return mimetype.split(';', 1)[0].trim().toLowerCase();
}

function filenameExtension(filename: string): string {
  const match = filename.trim().toLowerCase().match(/\.([^.]+)$/);
  return match?.[1] ?? '';
}

function hasPrefix(bytes: Uint8Array, prefix: number[]): boolean {
  return prefix.every((value, index) => bytes[index] === value);
}

function previewTooLargeError(maxBytes: number): AttachmentPreviewError {
  const maxMiB = Math.floor(maxBytes / 1024 / 1024);
  return new AttachmentPreviewError(`文本附件超过 ${maxMiB} MiB，无法在应用内预览`);
}
